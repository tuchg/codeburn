use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{debug, warn};

use crate::bash_utils::{extract_bash_commands, is_bash_tool};
use crate::models;
use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

const MODEL_DISPLAY_NAMES: &[(&str, &str)] = &[
    ("claude-opus-4-6", "Opus 4.6"),
    ("claude-opus-4-5", "Opus 4.5"),
    ("claude-opus-4-1", "Opus 4.1"),
    ("claude-opus-4", "Opus 4"),
    ("claude-sonnet-4-6", "Sonnet 4.6"),
    ("claude-sonnet-4-5", "Sonnet 4.5"),
    ("claude-sonnet-4", "Sonnet 4"),
    ("claude-3-7-sonnet", "Sonnet 3.7"),
    ("claude-3-5-sonnet", "Sonnet 3.5"),
    ("claude-haiku-4-5", "Haiku 4.5"),
    ("claude-3-5-haiku", "Haiku 3.5"),
];

const EDIT_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "FileEditTool",
    "FileWriteTool",
    "NotebookEdit",
    "cursor:edit",
];

#[derive(Debug, Deserialize)]
struct JournalEntry {
    #[serde(rename = "type")]
    entry_type: String,
    timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    #[allow(dead_code)]
    session_id: Option<String>,
    message: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    server_tool_use: Option<ServerToolUse>,
    speed: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServerToolUse {
    web_search_requests: Option<u64>,
}

pub struct ClaudeProvider {
    claude_dir: PathBuf,
}

impl ClaudeProvider {
    pub fn new(claude_dir: PathBuf) -> Self {
        Self { claude_dir }
    }

    pub fn default_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
            return PathBuf::from(dir);
        }
        dirs::home_dir()
            .map(|h| h.join(".claude"))
            .unwrap_or_else(|| PathBuf::from(".claude"))
    }

    fn desktop_sessions_dir() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            dirs::home_dir()
                .map(|h| {
                    h.join("Library/Application Support/Claude/local-agent-mode-sessions")
                })
                .unwrap_or_else(|| PathBuf::from("."))
        }
        #[cfg(target_os = "windows")]
        {
            dirs::home_dir()
                .map(|h| h.join("AppData/Roaming/Claude/local-agent-mode-sessions"))
                .unwrap_or_else(|| PathBuf::from("."))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            dirs::home_dir()
                .map(|h| h.join(".config/Claude/local-agent-mode-sessions"))
                .unwrap_or_else(|| PathBuf::from("."))
        }
    }
}

impl Provider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn display_name(&self) -> &str {
        "Claude"
    }

    fn model_display_name(&self, model: &str) -> String {
        let canonical = model.split('@').next().unwrap_or(model);
        let canonical = strip_date_suffix(canonical);
        for (key, name) in MODEL_DISPLAY_NAMES {
            if canonical.starts_with(key) {
                return name.to_string();
            }
        }
        canonical.to_string()
    }

    fn tool_display_name(&self, raw_tool: &str) -> String {
        raw_tool.to_string()
    }

    fn discover_sessions(&self) -> Vec<SessionSource> {
        let mut sources = Vec::new();

        // ~/.claude/projects/*
        let projects_dir = self.claude_dir.join("projects");
        if projects_dir.is_dir() {
            debug!(dir = %projects_dir.display(), "scanning claude projects directory");
            let entries = match fs::read_dir(&projects_dir) {
                Ok(e) => e,
                Err(e) => {
                    debug!(dir = %projects_dir.display(), error = %e, "claude projects directory not found");
                    return sources;
                }
            };
            for project_entry in entries.flatten() {
                if !project_entry.path().is_dir() {
                    continue;
                }
                let dir_name = project_entry.file_name().to_string_lossy().to_string();
                for file_path in collect_jsonl_files(&project_entry.path()) {
                    sources.push(SessionSource {
                        path: file_path.to_string_lossy().to_string(),
                        project: dir_name.clone(),
                        provider: "claude".to_string(),
                    });
                }
            }
        }

        // Claude Desktop local-agent-mode-sessions
        let desktop_dir = Self::desktop_sessions_dir();
        if desktop_dir.is_dir() {
            debug!(dir = %desktop_dir.display(), "scanning claude desktop sessions directory");
            for (project_path, project_name) in find_desktop_project_dirs(&desktop_dir, 0) {
                for file_path in collect_jsonl_files(&project_path) {
                    sources.push(SessionSource {
                        path: file_path.to_string_lossy().to_string(),
                        project: project_name.clone(),
                        provider: "claude".to_string(),
                    });
                }
            }
        }

        debug!(count = sources.len(), "discovered claude sessions");
        sources
    }

    fn parse_session(
        &self,
        source: &SessionSource,
        seen_keys: &mut HashSet<String>,
    ) -> Vec<ParsedProviderCall> {
        let content = match fs::read_to_string(&source.path) {
            Ok(c) => c,
            Err(e) => {
                warn!(path = %source.path, error = %e, "failed to read claude session file");
                return Vec::new();
            }
        };

        let entries: Vec<JournalEntry> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();

        if entries.is_empty() {
            return Vec::new();
        }

        let session_id = Path::new(&source.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut result: Vec<ParsedProviderCall> = Vec::new();
        let mut current_user_msg = String::new();
        // Track the timestamp of the last assistant entry that issued tool_use blocks,
        // and which result index it produced, so we can compute bash wall-clock time
        // when the following user entry returns the tool_result.
        let mut pending_tool_use_ts: Option<String> = None;
        let mut pending_call_idx: Option<usize> = None;

        for entry in &entries {
            if entry.entry_type == "user" {
                // If this user entry contains tool_results, compute bash execution time
                // as the delta between when the tool_use was issued and now.
                if let (Some(idx), Some(tool_ts)) =
                    (pending_call_idx.take(), pending_tool_use_ts.take())
                    && entry_has_tool_result(entry)
                    && let (Some(result_ts), Some(call)) =
                        (entry.timestamp.as_deref(), result.get_mut(idx))
                {
                    call.bash_duration_seconds +=
                        crate::timing::calculate_duration(&tool_ts, result_ts);
                }

                let text = get_user_message(entry);
                if !text.trim().is_empty() {
                    current_user_msg = text;
                }
            } else if entry.entry_type == "assistant" {
                if let Some(msg_id) = get_message_id(entry) {
                    if seen_keys.contains(&msg_id) {
                        continue;
                    }
                    seen_keys.insert(msg_id);
                }
                if let Some(call) =
                    parse_entry(entry, &current_user_msg, &session_id)
                {
                    let idx = result.len();
                    result.push(call);
                    // Track pending tool_use only if this assistant entry issued tool_use blocks.
                    if entry_has_tool_use(entry)
                        && let Some(ts) = &entry.timestamp
                    {
                        pending_tool_use_ts = Some(ts.clone());
                        pending_call_idx = Some(idx);
                    }
                }
            }
        }

        result
    }
}

fn strip_date_suffix(model: &str) -> &str {
    // Strip trailing -YYYYMMDD date suffix (e.g. "claude-sonnet-4-5-20250514")
    if model.len() >= 9 {
        let suffix = &model[model.len() - 9..];
        if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
            return &model[..model.len() - 9];
        }
    }
    model
}

fn collect_jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return files,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            files.push(path);
        } else if path.is_dir() {
            let subagents = path.join("subagents");
            if subagents.is_dir()
                && let Ok(sub_entries) = fs::read_dir(&subagents)
            {
                for sub in sub_entries.flatten() {
                    let sub_path = sub.path();
                    if sub_path.extension().is_some_and(|ext| ext == "jsonl") {
                        files.push(sub_path);
                    }
                }
            }
        }
    }

    files
}

/// Walk the Claude Desktop sessions base directory looking for `projects/` subdirectories,
/// matching the logic in the TypeScript claude.ts `findDesktopProjectDirs`.
fn find_desktop_project_dirs(base: &Path, depth: u32) -> Vec<(PathBuf, String)> {
    if depth > 8 {
        return Vec::new();
    }
    let mut results = Vec::new();
    let entries = match fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return results,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "node_modules" || name == ".git" {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if name == "projects" {
            if let Ok(project_entries) = fs::read_dir(&path) {
                for pe in project_entries.flatten() {
                    if pe.path().is_dir() {
                        let pname = pe.file_name().to_string_lossy().to_string();
                        results.push((pe.path(), pname));
                    }
                }
            }
        } else {
            results.extend(find_desktop_project_dirs(&path, depth + 1));
        }
    }
    results
}

fn entry_has_tool_use(entry: &JournalEntry) -> bool {
    let msg = match &entry.message {
        Some(m) => m,
        None => return false,
    };
    msg.get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter().any(|b| {
                b.get("type").and_then(|t| t.as_str()) == Some("tool_use")
            })
        })
        .unwrap_or(false)
}

fn entry_has_tool_result(entry: &JournalEntry) -> bool {
    let msg = match &entry.message {
        Some(m) => m,
        None => return false,
    };
    msg.get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter().any(|b| {
                b.get("type").and_then(|t| t.as_str()) == Some("tool_result")
            })
        })
        .unwrap_or(false)
}

fn get_user_message(entry: &JournalEntry) -> String {
    let msg = match &entry.message {
        Some(m) => m,
        None => return String::new(),
    };
    if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
        return String::new();
    }
    let content = match msg.get("content") {
        Some(c) => c,
        None => return String::new(),
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .filter_map(|b| {
                if b.get("type")?.as_str()? == "text" {
                    b.get("text")?.as_str().map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    String::new()
}

fn get_message_id(entry: &JournalEntry) -> Option<String> {
    if entry.entry_type != "assistant" {
        return None;
    }
    entry
        .message
        .as_ref()?
        .get("id")?
        .as_str()
        .map(String::from)
}

fn extract_tools(content: &[serde_json::Value]) -> Vec<String> {
    content
        .iter()
        .filter_map(|block| {
            if block.get("type")?.as_str()? == "tool_use" {
                block.get("name")?.as_str().map(String::from)
            } else {
                None
            }
        })
        .collect()
}

fn extract_bash_commands_from_content(content: &[serde_json::Value]) -> Vec<String> {
    content
        .iter()
        .filter_map(|block| {
            if block.get("type")?.as_str()? != "tool_use" {
                return None;
            }
            let name = block.get("name")?.as_str()?;
            if !is_bash_tool(name) {
                return None;
            }
            let command = block.get("input")?.get("command")?.as_str()?;
            Some(extract_bash_commands(command))
        })
        .flatten()
        .collect()
}

fn extract_file_paths(content: &[serde_json::Value]) -> Vec<String> {
    content
        .iter()
        .filter_map(|block| {
            if block.get("type")?.as_str()? != "tool_use" {
                return None;
            }
            let name = block.get("name")?.as_str()?;
            if !EDIT_TOOLS.contains(&name) {
                return None;
            }
            let input = block.get("input")?;
            input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect()
}

fn count_lines(s: &str) -> u64 {
    if s.is_empty() {
        return 0;
    }
    s.lines().count() as u64
}

fn extract_code_changes(content: &[serde_json::Value]) -> (u64, u64) {
    let mut added: u64 = 0;
    let mut removed: u64 = 0;

    for block in content {
        if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
            continue;
        }
        let name = match block.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let input = match block.get("input") {
            Some(i) => i,
            None => continue,
        };

        match name {
            "Edit" | "FileEditTool" => {
                let old = input
                    .get("old_string")
                    .or_else(|| input.get("old_text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new = input
                    .get("new_string")
                    .or_else(|| input.get("new_text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                removed += count_lines(old);
                added += count_lines(new);
            }
            "Write" | "FileWriteTool" => {
                let content_str = input
                    .get("content")
                    .or_else(|| input.get("file_text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                added += count_lines(content_str);
            }
            _ => {}
        }
    }

    (added, removed)
}

fn parse_entry(
    entry: &JournalEntry,
    user_message: &str,
    session_id: &str,
) -> Option<ParsedProviderCall> {
    if entry.entry_type != "assistant" {
        return None;
    }

    let msg = entry.message.as_ref()?;
    let model = msg.get("model")?.as_str()?;
    let usage_val = msg.get("usage")?;
    let usage: ApiUsage = serde_json::from_value(usage_val.clone()).ok()?;

    let speed = usage.speed.as_deref().unwrap_or("standard");
    let web_search_requests = usage
        .server_tool_use
        .as_ref()
        .and_then(|s| s.web_search_requests)
        .unwrap_or(0);

    let input_tokens = usage.input_tokens.unwrap_or(0);
    let output_tokens = usage.output_tokens.unwrap_or(0);
    let cache_creation_input_tokens = usage.cache_creation_input_tokens.unwrap_or(0);
    let cache_read_input_tokens = usage.cache_read_input_tokens.unwrap_or(0);

    let cost_usd = models::calculate_cost_with_overrides(
        &models::CostInput {
            model,
            input_tokens,
            output_tokens,
            cache_creation_tokens: cache_creation_input_tokens,
            cache_read_tokens: cache_read_input_tokens,
            web_search_requests,
            speed,
        },
        &[],
    );

    let content = msg
        .get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

    let tools = extract_tools(&content);
    let bash_commands = extract_bash_commands_from_content(&content);
    let file_paths = extract_file_paths(&content);
    let (lines_added, lines_removed) = extract_code_changes(&content);

    let deduplication_key = msg
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            format!("claude:{}", entry.timestamp.as_deref().unwrap_or(""))
        });

    Some(ParsedProviderCall {
        model: model.to_string(),
        input_tokens,
        output_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        web_search_requests,
        cost_usd,
        tools,
        bash_commands,
        file_paths,
        lines_added,
        lines_removed,
        speed: speed.to_string(),
        bash_duration_seconds: 0.0,
        timestamp: entry.timestamp.clone().unwrap_or_default(),
        deduplication_key,
        user_message: user_message.to_string(),
        session_id: session_id.to_string(),
    })
}
