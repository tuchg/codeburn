use std::collections::HashSet;
use std::path::PathBuf;

#[cfg(feature = "sqlite")]
use crate::bash_utils::extract_bash_commands;
use crate::models;
use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

const TOOL_NAME_MAP: &[(&str, &str)] = &[
    ("bash", "Bash"),
    ("read", "Read"),
    ("edit", "Edit"),
    ("write", "Write"),
    ("glob", "Glob"),
    ("grep", "Grep"),
    ("task", "Agent"),
    ("fetch", "WebFetch"),
    ("search", "WebSearch"),
    ("todo", "TodoWrite"),
    ("skill", "Skill"),
    ("patch", "Patch"),
];

fn map_tool_name(raw: &str) -> &str {
    for (key, mapped) in TOOL_NAME_MAP {
        if raw == *key {
            return mapped;
        }
    }
    raw
}

pub struct OpenCodeProvider {
    #[cfg(feature = "sqlite")]
    data_dir: PathBuf,
}

impl OpenCodeProvider {
    pub fn new(_data_dir: PathBuf) -> Self {
        Self {
            #[cfg(feature = "sqlite")]
            data_dir: _data_dir,
        }
    }

    pub fn default_dir() -> PathBuf {
        let base = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join(".local").join("share"))
                    .unwrap_or_else(|| PathBuf::from(".local/share"))
            });
        base.join("opencode")
    }
}

#[cfg(feature = "sqlite")]
fn sanitize(dir: &str) -> String {
    dir.trim_start_matches('/').replace('/', "-")
}

#[cfg(feature = "sqlite")]
fn parse_timestamp(raw: i64) -> String {
    let ms = if raw < 1_000_000_000_000 {
        raw * 1000
    } else {
        raw
    };
    let secs = ms / 1000;
    let nsecs = ((ms % 1000) * 1_000_000) as u32;
    chrono::DateTime::from_timestamp(secs, nsecs)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

impl Provider for OpenCodeProvider {
    fn name(&self) -> &str {
        "opencode"
    }

    fn display_name(&self) -> &str {
        "OpenCode"
    }

    fn model_display_name(&self, model: &str) -> String {
        // Strip provider prefix (e.g., "anthropic/claude-opus-4-6" -> "claude-opus-4-6")
        let stripped = model
            .find('/')
            .map(|i| &model[i + 1..])
            .unwrap_or(model);
        models::short_model_name(stripped)
    }

    fn tool_display_name(&self, raw_tool: &str) -> String {
        map_tool_name(raw_tool).to_string()
    }

    fn discover_sessions(&self) -> Vec<SessionSource> {
        #[cfg(feature = "sqlite")]
        {
            return discover_opencode_sessions(&self.data_dir);
        }
        #[cfg(not(feature = "sqlite"))]
        {
            Vec::new()
        }
    }

    fn parse_session(
        &self,
        source: &SessionSource,
        seen_keys: &mut HashSet<String>,
    ) -> Vec<ParsedProviderCall> {
        #[cfg(feature = "sqlite")]
        {
            return parse_opencode_session(source, seen_keys);
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (source, seen_keys);
            Vec::new()
        }
    }
}

#[cfg(feature = "sqlite")]
fn discover_opencode_sessions(data_dir: &std::path::Path) -> Vec<SessionSource> {
    use rusqlite::Connection;

    let db_files = find_db_files(data_dir);
    let mut sources = Vec::new();

    for db_path in db_files {
        let conn = match Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut stmt = match conn.prepare(
            "SELECT id, directory, title, time_created FROM session WHERE time_archived IS NULL AND parent_id IS NULL ORDER BY time_created DESC",
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => continue,
        };

        for row in rows.flatten() {
            let (id, directory, title) = row;
            let project = if let Some(dir) = &directory {
                if !dir.is_empty() {
                    sanitize(dir)
                } else {
                    sanitize(title.as_deref().unwrap_or("unknown"))
                }
            } else {
                sanitize(title.as_deref().unwrap_or("unknown"))
            };

            sources.push(SessionSource {
                path: format!("{}:{}", db_path.display(), id),
                project,
                provider: "opencode".to_string(),
            });
        }
    }

    sources
}

#[cfg(feature = "sqlite")]
fn find_db_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("opencode") && name.ends_with(".db")
        })
        .map(|e| e.path())
        .collect()
}

#[cfg(feature = "sqlite")]
fn parse_opencode_session(
    source: &SessionSource,
    seen_keys: &mut HashSet<String>,
) -> Vec<ParsedProviderCall> {
    use rusqlite::Connection;

    // Path format: dbpath:session_id (session IDs are UUIDs, no colons)
    let segments: Vec<&str> = source.path.split(':').collect();
    let session_id = segments.last().copied().unwrap_or("");
    let db_path = segments[..segments.len() - 1].join(":");

    let conn = match Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    // Validate schema
    let valid = conn
        .execute_batch("SELECT COUNT(*) FROM session LIMIT 1; SELECT COUNT(*) FROM message LIMIT 1;")
        .is_ok();
    if !valid {
        return Vec::new();
    }

    // Load messages
    let mut msg_stmt = match conn.prepare(
        "SELECT id, time_created, data FROM message WHERE session_id = ? ORDER BY time_created ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let messages: Vec<(String, i64, String)> = msg_stmt
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .collect();

    // Load parts
    let mut part_stmt = match conn.prepare(
        "SELECT message_id, data FROM part WHERE session_id = ? ORDER BY message_id, id",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let parts: Vec<(String, String)> = part_stmt
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .collect();

    // Group parts by message
    let mut parts_by_msg: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for (msg_id, data) in &parts {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
            parts_by_msg.entry(msg_id.clone()).or_default().push(parsed);
        }
    }

    let mut current_user_message = String::new();
    let mut results = Vec::new();

    for (msg_id, time_created, data_str) in &messages {
        let data: serde_json::Value = match serde_json::from_str(data_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role = data.get("role").and_then(|v| v.as_str()).unwrap_or("");

        if role == "user" {
            let msg_parts = parts_by_msg.get(msg_id).cloned().unwrap_or_default();
            let text_parts: Vec<String> = msg_parts
                .iter()
                .filter(|p| p.get("type").and_then(|v| v.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|v| v.as_str()).map(String::from))
                .filter(|s| !s.is_empty())
                .collect();
            if !text_parts.is_empty() {
                current_user_message = text_parts.join(" ");
            }
            continue;
        }

        if role != "assistant" {
            continue;
        }

        let input = data
            .get("tokens")
            .and_then(|t| t.get("input"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output = data
            .get("tokens")
            .and_then(|t| t.get("output"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let reasoning = data
            .get("tokens")
            .and_then(|t| t.get("reasoning"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_read = data
            .get("tokens")
            .and_then(|t| t.get("cache"))
            .and_then(|c| c.get("read"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_write = data
            .get("tokens")
            .and_then(|t| t.get("cache"))
            .and_then(|c| c.get("write"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let data_cost = data.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let all_zero =
            input == 0 && output == 0 && reasoning == 0 && cache_read == 0 && cache_write == 0;
        if all_zero && data_cost == 0.0 {
            continue;
        }

        let msg_parts = parts_by_msg.get(msg_id).cloned().unwrap_or_default();
        let tool_parts: Vec<&serde_json::Value> = msg_parts
            .iter()
            .filter(|p| p.get("type").and_then(|v| v.as_str()) == Some("tool"))
            .collect();

        let tools: Vec<String> = tool_parts
            .iter()
            .filter_map(|p| {
                let raw = p.get("tool").and_then(|v| v.as_str()).unwrap_or("");
                if raw.is_empty() {
                    None
                } else {
                    Some(map_tool_name(raw).to_string())
                }
            })
            .collect();

        let bash_cmds: Vec<String> = tool_parts
            .iter()
            .filter(|p| p.get("tool").and_then(|v| v.as_str()) == Some("bash"))
            .filter_map(|p| {
                p.get("state")
                    .and_then(|s| s.get("input"))
                    .and_then(|i| i.get("command"))
                    .and_then(|v| v.as_str())
            })
            .flat_map(extract_bash_commands)
            .collect();

        let dedup_key = format!("opencode:{}:{}", session_id, msg_id);
        if seen_keys.contains(&dedup_key) {
            continue;
        }
        seen_keys.insert(dedup_key.clone());

        let model = data
            .get("modelID")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut cost_usd = models::calculate_cost(
            &model,
            input,
            output + reasoning,
            cache_write,
            cache_read,
            0,
            "standard",
        );

        if cost_usd == 0.0 && data_cost > 0.0 {
            cost_usd = data_cost;
        }

        results.push(ParsedProviderCall {
            model,
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: cache_write,
            cache_read_input_tokens: cache_read,
            cached_input_tokens: cache_read,
            reasoning_tokens: reasoning,
            web_search_requests: 0,
            cost_usd,
            tools,
            bash_commands: bash_cmds,
            timestamp: parse_timestamp(*time_created),
            deduplication_key: dedup_key,
            user_message: std::mem::take(&mut current_user_message),
            session_id: session_id.to_string(),
        });
    }

    results
}
