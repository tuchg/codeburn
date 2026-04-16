use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use tracing::debug;

use crate::models;
use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

const MODEL_DISPLAY_NAMES: &[(&str, &str)] = &[
    ("gpt-5.3-codex", "GPT-5.3 Codex"),
    ("gpt-5.4-mini", "GPT-5.4 Mini"),
    ("gpt-5.4", "GPT-5.4"),
    ("gpt-5", "GPT-5"),
    ("gpt-4o-mini", "GPT-4o Mini"),
    ("gpt-4o", "GPT-4o"),
];

const TOOL_NAME_MAP: &[(&str, &str)] = &[
    ("exec_command", "Bash"),
    ("read_file", "Read"),
    ("write_file", "Edit"),
    ("apply_diff", "Edit"),
    ("apply_patch", "Edit"),
    ("spawn_agent", "Agent"),
    ("close_agent", "Agent"),
    ("wait_agent", "Agent"),
    ("read_dir", "Glob"),
];

pub struct CodexProvider {
    codex_dir: PathBuf,
}

impl CodexProvider {
    pub fn new(codex_dir: PathBuf) -> Self {
        Self { codex_dir }
    }

    pub fn default_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("CODEX_HOME") {
            return PathBuf::from(dir);
        }
        dirs::home_dir()
            .map(|h| h.join(".codex"))
            .unwrap_or_else(|| PathBuf::from(".codex"))
    }
}

fn map_tool_name(raw: &str) -> &str {
    for (key, mapped) in TOOL_NAME_MAP {
        if raw == *key {
            return mapped;
        }
    }
    raw
}

impl Provider for CodexProvider {
    fn name(&self) -> &str {
        "codex"
    }

    fn display_name(&self) -> &str {
        "Codex"
    }

    fn model_display_name(&self, model: &str) -> String {
        for (key, name) in MODEL_DISPLAY_NAMES {
            if model.starts_with(key) {
                return name.to_string();
            }
        }
        model.to_string()
    }

    fn tool_display_name(&self, raw_tool: &str) -> String {
        map_tool_name(raw_tool).to_string()
    }

    fn discover_sessions(&self) -> Vec<SessionSource> {
        let sessions_dir = self.codex_dir.join("sessions");
        if !sessions_dir.is_dir() {
            debug!(dir = %sessions_dir.display(), "codex sessions directory not found");
            return Vec::new();
        }

        let mut sources = Vec::new();

        let years = match fs::read_dir(&sessions_dir) {
            Ok(e) => e,
            Err(_) => return sources,
        };

        for year_entry in years.flatten() {
            let year_name = year_entry.file_name().to_string_lossy().to_string();
            if year_name.len() != 4
                || !year_name.chars().all(|c| c.is_ascii_digit())
                || !year_entry.path().is_dir()
            {
                continue;
            }

            let months = match fs::read_dir(year_entry.path()) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for month_entry in months.flatten() {
                let month_name = month_entry.file_name().to_string_lossy().to_string();
                if month_name.len() != 2
                    || !month_name.chars().all(|c| c.is_ascii_digit())
                    || !month_entry.path().is_dir()
                {
                    continue;
                }

                let days = match fs::read_dir(month_entry.path()) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                for day_entry in days.flatten() {
                    let day_name = day_entry.file_name().to_string_lossy().to_string();
                    if day_name.len() != 2
                        || !day_name.chars().all(|c| c.is_ascii_digit())
                        || !day_entry.path().is_dir()
                    {
                        continue;
                    }

                    let files = match fs::read_dir(day_entry.path()) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    for file_entry in files.flatten() {
                        let fname = file_entry.file_name().to_string_lossy().to_string();
                        if !fname.starts_with("rollout-") || !fname.ends_with(".jsonl") {
                            continue;
                        }
                        let file_path = file_entry.path();
                        if !file_path.is_file() {
                            continue;
                        }

                        if let Some((project, _valid)) =
                            validate_codex_session(&file_path)
                        {
                            sources.push(SessionSource {
                                path: file_path.to_string_lossy().to_string(),
                                project,
                                provider: "codex".to_string(),
                            });
                        }
                    }
                }
            }
        }

        sources
    }

    fn parse_session(
        &self,
        source: &SessionSource,
        seen_keys: &mut HashSet<String>,
    ) -> Vec<ParsedProviderCall> {
        let file_path = Path::new(&source.path);
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() {
            return Vec::new();
        }

        // Parse session_meta from first line
        let first: serde_json::Value = match serde_json::from_str(lines[0]) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        if first.get("type").and_then(|v| v.as_str()) != Some("session_meta") {
            return Vec::new();
        }
        let payload = match first.get("payload") {
            Some(p) => p,
            None => return Vec::new(),
        };
        let originator = payload
            .get("originator")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !originator.to_lowercase().starts_with("codex") {
            return Vec::new();
        }

        let session_id = payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                file_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
        let session_model = payload
            .get("model")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut prev_cumulative_total: u64 = 0;
        let mut prev_input: u64 = 0;
        let mut prev_cached: u64 = 0;
        let mut prev_output: u64 = 0;
        let mut prev_reasoning: u64 = 0;
        let mut pending_tools: Vec<String> = Vec::new();
        let mut pending_user_msg = String::new();
        let mut calls = Vec::new();

        for line in &lines[1..] {
            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

            if entry_type == "response_item"
                && let Some(p) = entry.get("payload")
            {
                let p_type = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if p_type == "function_call" {
                    let raw_name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    pending_tools.push(map_tool_name(raw_name).to_string());
                    continue;
                }
                if p_type == "message" {
                    let role = p.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    if role == "user" {
                        if let Some(arr) = p.get("content").and_then(|c| c.as_array()) {
                            let texts: Vec<String> = arr
                                .iter()
                                .filter_map(|c| {
                                    if c.get("type")?.as_str()? == "input_text" {
                                        c.get("text")?.as_str().map(String::from)
                                    } else {
                                        None
                                    }
                                })
                                .filter(|s| !s.is_empty())
                                .collect();
                            if !texts.is_empty() {
                                pending_user_msg = texts.join(" ");
                            }
                        }
                        continue;
                    }
                }
            }

            if entry_type == "event_msg" {
                let payload = match entry.get("payload") {
                    Some(p) => p,
                    None => continue,
                };
                if payload.get("type").and_then(|v| v.as_str()) != Some("token_count") {
                    continue;
                }
                let info = match payload.get("info") {
                    Some(i) => i,
                    None => continue,
                };

                let cumulative_total = info
                    .get("total_token_usage")
                    .and_then(|t| t.get("total_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                if cumulative_total > 0 && cumulative_total == prev_cumulative_total {
                    continue;
                }
                prev_cumulative_total = cumulative_total;

                let (input_tokens, cached_input, output_tokens, reasoning_tokens);

                if let Some(last) = info.get("last_token_usage") {
                    input_tokens = last
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    cached_input = last
                        .get("cached_input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    output_tokens = last
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    reasoning_tokens = last
                        .get("reasoning_output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                } else if cumulative_total > 0 {
                    let total = match info.get("total_token_usage") {
                        Some(t) => t,
                        None => continue,
                    };
                    let ti = total
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let tc = total
                        .get("cached_input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let to = total
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let tr = total
                        .get("reasoning_output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    input_tokens = ti.saturating_sub(prev_input);
                    cached_input = tc.saturating_sub(prev_cached);
                    output_tokens = to.saturating_sub(prev_output);
                    reasoning_tokens = tr.saturating_sub(prev_reasoning);
                    prev_input = ti;
                    prev_cached = tc;
                    prev_output = to;
                    prev_reasoning = tr;
                } else {
                    continue;
                }

                let total_tokens = input_tokens + cached_input + output_tokens + reasoning_tokens;
                if total_tokens == 0 {
                    continue;
                }

                // OpenAI includes cached tokens inside input_tokens; normalize
                let uncached_input = input_tokens.saturating_sub(cached_input);

                let model = info
                    .get("model")
                    .or_else(|| info.get("model_name"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| session_model.clone())
                    .unwrap_or_else(|| "gpt-5".to_string());

                let timestamp = entry
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let dedup_key = format!(
                    "codex:{}:{}:{}",
                    file_path.display(),
                    timestamp,
                    cumulative_total
                );

                if seen_keys.contains(&dedup_key) {
                    continue;
                }
                seen_keys.insert(dedup_key.clone());

                let cost_usd = models::calculate_cost(
                    &model,
                    uncached_input,
                    output_tokens + reasoning_tokens,
                    0,
                    cached_input,
                    0,
                    "standard",
                );

                calls.push(ParsedProviderCall {
                    model,
                    input_tokens: uncached_input,
                    output_tokens,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: cached_input,
                    cached_input_tokens: cached_input,
                    reasoning_tokens,
                    web_search_requests: 0,
                    cost_usd,
                    tools: std::mem::take(&mut pending_tools),
                    bash_commands: vec![],
                    file_paths: vec![],
                    lines_added: 0,
                    lines_removed: 0,
                    speed: String::new(),
                    bash_duration_seconds: 0.0,
                    timestamp,
                    deduplication_key: dedup_key,
                    user_message: std::mem::take(&mut pending_user_msg),
                    session_id: session_id.clone(),
                });
            }
        }

        calls
    }
}

fn validate_codex_session(file_path: &Path) -> Option<(String, bool)> {
    let content = fs::read_to_string(file_path).ok()?;
    let first_line = content.lines().next()?;
    let entry: serde_json::Value = serde_json::from_str(first_line.trim()).ok()?;

    if entry.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let payload = entry.get("payload")?;
    let originator = payload.get("originator")?.as_str()?;
    if !originator.to_lowercase().starts_with("codex") {
        return None;
    }

    let cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let project = cwd.trim_start_matches('/').replace('/', "-");

    Some((project, true))
}
