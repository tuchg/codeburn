use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use tracing::debug;

use crate::models;
use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

const MODEL_DISPLAY_NAMES: &[(&str, &str)] = &[
    ("gpt-4.1-mini", "GPT-4.1 Mini"),
    ("gpt-4.1-nano", "GPT-4.1 Nano"),
    ("gpt-4.1", "GPT-4.1"),
    ("gpt-4o-mini", "GPT-4o Mini"),
    ("gpt-4o", "GPT-4o"),
    ("gpt-5-mini", "GPT-5 Mini"),
    ("gpt-5", "GPT-5"),
    ("claude-sonnet-4-5", "Sonnet 4.5"),
    ("claude-sonnet-4", "Sonnet 4"),
    ("claude-3-7-sonnet", "Sonnet 3.7"),
    ("claude-3-5-sonnet", "Sonnet 3.5"),
    ("o3", "o3"),
    ("o4-mini", "o4-mini"),
];

const TOOL_NAME_MAP: &[(&str, &str)] = &[
    ("bash", "Bash"),
    ("read_file", "Read"),
    ("write_file", "Edit"),
    ("edit_file", "Edit"),
    ("create_file", "Write"),
    ("delete_file", "Edit"),
    ("search_files", "Grep"),
    ("find_files", "Glob"),
    ("list_directory", "LS"),
    ("web_search", "WebSearch"),
    ("fetch_webpage", "WebFetch"),
    ("github_repo", "GitHub"),
];

fn map_tool_name(raw: &str) -> &str {
    for (key, mapped) in TOOL_NAME_MAP {
        if raw == *key {
            return mapped;
        }
    }
    raw
}

pub struct CopilotProvider {
    session_state_dir: PathBuf,
}

impl CopilotProvider {
    pub fn new(session_state_dir: PathBuf) -> Self {
        Self { session_state_dir }
    }

    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .map(|h| h.join(".copilot").join("session-state"))
            .unwrap_or_else(|| PathBuf::from(".copilot/session-state"))
    }
}

impl Provider for CopilotProvider {
    fn name(&self) -> &str {
        "copilot"
    }

    fn display_name(&self) -> &str {
        "Copilot"
    }

    fn model_display_name(&self, model: &str) -> String {
        // Sort by key length descending so more specific keys match first.
        // Use exact match or `{key}-` prefix to avoid "gpt-4.1" swallowing "gpt-4.1-mini".
        let mut entries: Vec<_> = MODEL_DISPLAY_NAMES.iter().collect();
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (key, name) in entries {
            if model == *key || model.starts_with(&format!("{}-", key)) {
                return name.to_string();
            }
        }
        model.to_string()
    }

    fn tool_display_name(&self, raw_tool: &str) -> String {
        map_tool_name(raw_tool).to_string()
    }

    fn discover_sessions(&self) -> Vec<SessionSource> {
        let session_dirs = match fs::read_dir(&self.session_state_dir) {
            Ok(e) => e,
            Err(e) => {
                debug!(dir = %self.session_state_dir.display(), error = %e, "copilot session directory not readable");
                return Vec::new();
            }
        };

        let mut sources = Vec::new();

        for entry in session_dirs.flatten() {
            let session_id = entry.file_name().to_string_lossy().to_string();
            let events_path = entry.path().join("events.jsonl");
            if !events_path.is_file() {
                continue;
            }

            // Extract project name from workspace.yaml cwd field
            let project = read_project_from_workspace(&entry.path())
                .unwrap_or_else(|| session_id.clone());

            sources.push(SessionSource {
                path: events_path.to_string_lossy().to_string(),
                project,
                provider: "copilot".to_string(),
            });
        }

        sources
    }

    fn parse_session(
        &self,
        source: &SessionSource,
        seen_keys: &mut HashSet<String>,
    ) -> Vec<ParsedProviderCall> {
        let content = match fs::read_to_string(&source.path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let session_id = std::path::Path::new(&source.path)
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut current_model = "gpt-4.1".to_string();
        let mut pending_user_message = String::new();
        let mut calls = Vec::new();

        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            let event: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let data = match event.get("data") {
                Some(d) => d,
                None => continue,
            };
            let timestamp = event
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            match event_type {
                "session.model_change" => {
                    if let Some(model) = data.get("newModel").and_then(|v| v.as_str()) {
                        current_model = model.to_string();
                    }
                }
                "user.message" => {
                    if let Some(content) = data.get("content").and_then(|v| v.as_str()) {
                        pending_user_message = content.to_string();
                    }
                }
                "assistant.message" => {
                    let output_tokens = data
                        .get("outputTokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if output_tokens == 0 {
                        continue;
                    }

                    let message_id = data
                        .get("messageId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let dedup_key = format!("copilot:{}:{}", session_id, message_id);
                    if seen_keys.contains(&dedup_key) {
                        continue;
                    }
                    seen_keys.insert(dedup_key.clone());

                    let tools: Vec<String> = data
                        .get("toolRequests")
                        .and_then(|tr| tr.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|t| t.get("name")?.as_str())
                                .map(|n| map_tool_name(n).to_string())
                                .collect()
                        })
                        .unwrap_or_default();

                    let cost_usd = models::calculate_cost(
                        &current_model,
                        0,
                        output_tokens,
                        0,
                        0,
                        0,
                        "standard",
                    );

                    calls.push(ParsedProviderCall {
                        model: current_model.clone(),
                        input_tokens: 0,
                        output_tokens,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                        cached_input_tokens: 0,
                        reasoning_tokens: 0,
                        web_search_requests: 0,
                        cost_usd,
                        tools,
                        bash_commands: vec![],
                        file_paths: vec![],
                        lines_added: 0,
                        lines_removed: 0,
                        speed: String::new(),
                        bash_duration_seconds: 0.0,
                        timestamp,
                        deduplication_key: dedup_key,
                        user_message: std::mem::take(&mut pending_user_message),
                        session_id: session_id.clone(),
                    });
                }
                _ => {}
            }
        }

        calls
    }
}

fn read_project_from_workspace(session_dir: &std::path::Path) -> Option<String> {
    let yaml_path = session_dir.join("workspace.yaml");
    let file = fs::File::open(&yaml_path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix("cwd:") {
            let cwd = rest.trim();
            if !cwd.is_empty() {
                return Some(
                    cwd.trim_start_matches('/')
                        .replace('/', "-"),
                );
            }
        }
    }
    None
}
