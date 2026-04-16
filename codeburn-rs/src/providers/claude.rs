use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::bash_utils::{extract_bash_commands, is_bash_tool};
use crate::models;
use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

const SHORT_NAMES: &[(&str, &str)] = &[
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

pub struct ClaudeProvider {
    claude_dir: PathBuf,
}

impl ClaudeProvider {
    pub fn new(claude_dir: PathBuf) -> Self {
        Self { claude_dir }
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
        let canonical = models::get_canonical_name(model);
        for (key, name) in SHORT_NAMES {
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

        let projects_dir = self.claude_dir.join("projects");
        if let Ok(entries) = fs::read_dir(&projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    sources.push(SessionSource {
                        path: path.to_string_lossy().to_string(),
                        project: dir_name,
                        provider: "claude".to_string(),
                    });
                }
            }
        }

        // Desktop sessions (Claude Desktop local-agent-mode-sessions)
        let desktop_dir = get_desktop_sessions_dir();
        let desktop_project_dirs = find_desktop_project_dirs(&desktop_dir, 0);
        for dir_path in desktop_project_dirs {
            let name = dir_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            sources.push(SessionSource {
                path: dir_path.to_string_lossy().to_string(),
                project: name,
                provider: "claude".to_string(),
            });
        }

        sources
    }

    fn parse_session(
        &self,
        source: &SessionSource,
        seen_keys: &mut HashSet<String>,
    ) -> Vec<ParsedProviderCall> {
        let dir_path = Path::new(&source.path);
        let jsonl_files = collect_jsonl_files(dir_path);
        let mut calls = Vec::new();

        for file_path in jsonl_files {
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut pending_user_msg = String::new();

            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                let entry: serde_json::Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

                if entry_type == "user" {
                    if let Some(msg) = entry.get("message")
                        && msg.get("role").and_then(|r| r.as_str()) == Some("user")
                    {
                        let text = extract_user_message_text(msg);
                        if !text.trim().is_empty() {
                            pending_user_msg = text;
                        }
                    }
                    continue;
                }

                if entry_type != "assistant" {
                    continue;
                }

                let msg = match entry.get("message") {
                    Some(m) => m,
                    None => continue,
                };

                let model = match msg.get("model").and_then(|v| v.as_str()) {
                    Some(m) => m,
                    None => continue,
                };

                let usage_val = match msg.get("usage") {
                    Some(u) => u,
                    None => continue,
                };

                let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let dedup_key = if msg_id.is_empty() {
                    let ts = entry.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    format!("claude:{}", ts)
                } else {
                    msg_id.to_string()
                };

                if seen_keys.contains(&dedup_key) {
                    continue;
                }
                seen_keys.insert(dedup_key.clone());

                let input_tokens = usage_val
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output_tokens = usage_val
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_creation = usage_val
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_read = usage_val
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let web_search = usage_val
                    .get("server_tool_use")
                    .and_then(|s| s.get("web_search_requests"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let speed = usage_val
                    .get("speed")
                    .and_then(|v| v.as_str())
                    .unwrap_or("standard");

                let content_arr = msg
                    .get("content")
                    .and_then(|c| c.as_array())
                    .cloned()
                    .unwrap_or_default();

                let tools = extract_tools(&content_arr);
                let bash_cmds = extract_bash_commands_from_content(&content_arr);

                let cost_usd = models::calculate_cost(
                    model,
                    input_tokens,
                    output_tokens,
                    cache_creation,
                    cache_read,
                    web_search,
                    speed,
                );

                let timestamp = entry
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let session_id = entry
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| {
                        file_path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default()
                    });

                calls.push(ParsedProviderCall {
                    provider: "claude".to_string(),
                    model: model.to_string(),
                    input_tokens,
                    output_tokens,
                    cache_creation_input_tokens: cache_creation,
                    cache_read_input_tokens: cache_read,
                    cached_input_tokens: 0,
                    reasoning_tokens: 0,
                    web_search_requests: web_search,
                    cost_usd,
                    tools,
                    bash_commands: bash_cmds,
                    timestamp,
                    speed: speed.to_string(),
                    deduplication_key: dedup_key,
                    user_message: std::mem::take(&mut pending_user_msg),
                    session_id,
                });
            }
        }

        calls
    }
}

fn extract_user_message_text(msg: &serde_json::Value) -> String {
    if let Some(content) = msg.get("content") {
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
    }
    String::new()
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

fn get_desktop_sessions_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
            .join("local-agent-mode-sessions")
    } else if cfg!(target_os = "windows") {
        home.join("AppData")
            .join("Roaming")
            .join("Claude")
            .join("local-agent-mode-sessions")
    } else {
        home.join(".config")
            .join("Claude")
            .join("local-agent-mode-sessions")
    }
}

fn find_desktop_project_dirs(base: &Path, depth: usize) -> Vec<PathBuf> {
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
        let full = entry.path();
        if !full.is_dir() {
            continue;
        }
        if name == "projects" {
            if let Ok(project_dirs) = fs::read_dir(&full) {
                for pd in project_dirs.flatten() {
                    if pd.path().is_dir() {
                        results.push(pd.path());
                    }
                }
            }
        } else {
            results.extend(find_desktop_project_dirs(&full, depth + 1));
        }
    }

    results
}
