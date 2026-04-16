use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::models;
use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

pub struct GeminiProvider {
    gemini_dir: PathBuf,
}

impl GeminiProvider {
    pub fn new(gemini_dir: PathBuf) -> Self {
        Self { gemini_dir }
    }

    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .map(|h| h.join(".gemini"))
            .unwrap_or_else(|| PathBuf::from(".gemini"))
    }
}

fn model_display_name_for(model: &str) -> String {
    model
        .trim_end_matches("-preview")
        .replace("gemini-", "Gemini ")
        .replace('-', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Provider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn display_name(&self) -> &str {
        "Gemini"
    }

    fn model_display_name(&self, model: &str) -> String {
        model_display_name_for(model)
    }

    fn tool_display_name(&self, raw_tool: &str) -> String {
        raw_tool.to_string()
    }

    fn discover_sessions(&self) -> Vec<SessionSource> {
        let projects_file = self.gemini_dir.join("projects.json");
        let content = match fs::read_to_string(&projects_file) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let data: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let projects = match data.get("projects").and_then(|p| p.as_object()) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut sources = Vec::new();

        for (_project_path, project_name_val) in projects {
            let project_name = match project_name_val.as_str() {
                Some(n) => n,
                None => continue,
            };
            let chats_dir = self
                .gemini_dir
                .join("tmp")
                .join(project_name)
                .join("chats");
            let entries = match fs::read_dir(&chats_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") && path.is_file() {
                    sources.push(SessionSource {
                        path: path.to_string_lossy().to_string(),
                        project: project_name.to_string(),
                        provider: "gemini".to_string(),
                    });
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
        let content = match fs::read_to_string(&source.path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let session: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let session_id = session
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let messages = match session.get("messages").and_then(|m| m.as_array()) {
            Some(m) => m,
            None => return Vec::new(),
        };

        let mut last_user_message = String::new();
        let mut calls = Vec::new();

        for msg in messages {
            let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

            if msg_type == "user" {
                let content = msg.get("content");
                last_user_message = if let Some(s) = content.and_then(|c| c.as_str()) {
                    s.to_string()
                } else if let Some(arr) = content.and_then(|c| c.as_array()) {
                    arr.iter()
                        .filter_map(|c| c.get("text")?.as_str().map(String::from))
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    String::new()
                };
                continue;
            }

            if msg_type != "gemini" {
                continue;
            }

            let tokens = match msg.get("tokens") {
                Some(t) => t,
                None => continue,
            };

            let message_id = msg
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let dedup_key = format!("gemini:{}:{}", session_id, message_id);
            if seen_keys.contains(&dedup_key) {
                continue;
            }
            seen_keys.insert(dedup_key.clone());

            let raw_input = tokens
                .get("input")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cached = tokens
                .get("cached")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output_tokens = tokens
                .get("output")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let reasoning_tokens = tokens
                .get("thoughts")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let input_tokens = raw_input.saturating_sub(cached);
            let model = msg
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("gemini-3-flash")
                .to_string();
            let timestamp = msg
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let tools: Vec<String> = msg
                .get("toolCalls")
                .and_then(|tc| tc.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.get("name")?.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let cost_usd = models::calculate_cost(
                &model,
                input_tokens,
                output_tokens + reasoning_tokens,
                0,
                cached,
                0,
                "standard",
            );

            calls.push(ParsedProviderCall {
                model,
                input_tokens,
                output_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: cached,
                cached_input_tokens: cached,
                reasoning_tokens,
                web_search_requests: 0,
                cost_usd,
                tools,
                bash_commands: vec![],
                timestamp,
                deduplication_key: dedup_key,
                user_message: last_user_message.clone(),
                session_id: session_id.clone(),
            });
        }

        calls
    }
}
