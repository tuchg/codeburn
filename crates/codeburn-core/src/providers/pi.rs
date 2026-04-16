use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use tracing::debug;

use crate::bash_utils::{extract_bash_commands, is_bash_tool};
use crate::models;
use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

const MODEL_DISPLAY_NAMES: &[(&str, &str)] = &[
    ("gpt-5.4-mini", "GPT-5.4 Mini"),
    ("gpt-5.4", "GPT-5.4"),
    ("gpt-5", "GPT-5"),
    ("gpt-4o-mini", "GPT-4o Mini"),
    ("gpt-4o", "GPT-4o"),
];

pub struct PiProvider {
    sessions_dir: PathBuf,
}

impl PiProvider {
    pub fn new(sessions_dir: PathBuf) -> Self {
        Self { sessions_dir }
    }

    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .map(|h| h.join(".pi").join("agent").join("sessions"))
            .unwrap_or_else(|| PathBuf::from(".pi/agent/sessions"))
    }
}

fn read_first_entry(file_path: &Path) -> Option<serde_json::Value> {
    let file = fs::File::open(file_path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

impl Provider for PiProvider {
    fn name(&self) -> &str {
        "pi"
    }

    fn display_name(&self) -> &str {
        "Pi"
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
        raw_tool.to_string()
    }

    fn discover_sessions(&self) -> Vec<SessionSource> {
        let project_dirs = match fs::read_dir(&self.sessions_dir) {
            Ok(e) => e,
            Err(e) => {
                debug!(dir = %self.sessions_dir.display(), error = %e, "pi sessions directory not readable");
                return Vec::new();
            }
        };

        let mut sources = Vec::new();

        for dir_entry in project_dirs.flatten() {
            let dir_path = dir_entry.path();
            if !dir_path.is_dir() {
                continue;
            }

            let files = match fs::read_dir(&dir_path) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for file_entry in files.flatten() {
                let file_path = file_entry.path();
                if file_path.extension().is_none_or(|e| e != "jsonl") {
                    continue;
                }
                if !file_path.is_file() {
                    continue;
                }

                // Validate: first line must have type == "session"
                let first = match read_first_entry(&file_path) {
                    Some(v) => v,
                    None => continue,
                };
                if first.get("type").and_then(|v| v.as_str()) != Some("session") {
                    continue;
                }

                let dir_name = dir_entry.file_name().to_string_lossy().to_string();
                let cwd = first
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&dir_name);
                let project = Path::new(cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(cwd)
                    .to_string();

                sources.push(SessionSource {
                    path: file_path.to_string_lossy().to_string(),
                    project,
                    provider: "pi".to_string(),
                });
            }
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

        let file_path = Path::new(&source.path);
        let mut session_id = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut pending_user_message = String::new();
        let mut calls = Vec::new();

        for (line_idx, line) in content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .enumerate()
        {
            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

            if entry_type == "session" {
                if let Some(id) = entry.get("id").and_then(|v| v.as_str()) {
                    session_id = id.to_string();
                }
                continue;
            }

            if entry_type != "message" {
                continue;
            }

            let msg = match entry.get("message") {
                Some(m) => m,
                None => continue,
            };

            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");

            if role == "user" {
                let texts: Vec<String> = msg
                    .get("content")
                    .and_then(|c| c.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter(|c| {
                                c.get("type").and_then(|t| t.as_str()) == Some("text")
                            })
                            .filter_map(|c| c.get("text")?.as_str().map(String::from))
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                if !texts.is_empty() {
                    pending_user_message = texts.join(" ");
                }
                continue;
            }

            if role != "assistant" {
                continue;
            }

            let usage = match msg.get("usage") {
                Some(u) => u,
                None => continue,
            };

            let input = usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
            let output = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
            if input == 0 && output == 0 {
                continue;
            }

            let cache_read = usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0);
            let cache_write = usage
                .get("cacheWrite")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let model = msg
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("gpt-5")
                .to_string();
            let response_id = msg
                .get("responseId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let entry_id = entry.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let entry_ts = entry
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let dedup_suffix = if !response_id.is_empty() {
                response_id.to_string()
            } else if !entry_id.is_empty() {
                entry_id.to_string()
            } else if !entry_ts.is_empty() {
                entry_ts.to_string()
            } else {
                line_idx.to_string()
            };
            let dedup_key = format!("pi:{}:{}", source.path, dedup_suffix);

            if seen_keys.contains(&dedup_key) {
                continue;
            }
            seen_keys.insert(dedup_key.clone());

            let content_arr = msg
                .get("content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();

            let tools: Vec<String> = content_arr
                .iter()
                .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("toolCall"))
                .filter_map(|c| c.get("name")?.as_str().map(String::from))
                .collect();

            let bash_commands: Vec<String> = content_arr
                .iter()
                .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("toolCall"))
                .filter_map(|c| {
                    let name = c.get("name")?.as_str()?;
                    if !is_bash_tool(name) {
                        return None;
                    }
                    let cmd = c.get("arguments")?.get("command")?.as_str()?;
                    Some(extract_bash_commands(cmd))
                })
                .flatten()
                .collect();

            let timestamp = entry_ts.to_string();
            let cost_usd =
                models::calculate_cost(&model, input, output, cache_write, cache_read, 0, "standard");

            calls.push(ParsedProviderCall {
                model,
                input_tokens: input,
                output_tokens: output,
                cache_creation_input_tokens: cache_write,
                cache_read_input_tokens: cache_read,
                cached_input_tokens: cache_read,
                reasoning_tokens: 0,
                web_search_requests: 0,
                cost_usd,
                tools,
                bash_commands,
                file_paths: vec![],
                lines_added: 0,
                lines_removed: 0,
                speed: String::new(),
                timestamp,
                deduplication_key: dedup_key,
                user_message: std::mem::take(&mut pending_user_message),
                session_id: session_id.clone(),
            });
        }

        calls
    }
}
