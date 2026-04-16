use std::collections::HashSet;
use std::path::PathBuf;

use tracing::debug;

use crate::providers::types::{ParsedProviderCall, Provider, SessionSource};

const MODEL_DISPLAY_NAMES: &[(&str, &str)] = &[
    ("claude-4.5-opus-high-thinking", "Opus 4.5 (Thinking)"),
    ("claude-4-opus", "Opus 4"),
    ("claude-4-sonnet-thinking", "Sonnet 4 (Thinking)"),
    ("claude-4.5-sonnet-thinking", "Sonnet 4.5 (Thinking)"),
    ("claude-4.6-sonnet", "Sonnet 4.6"),
    ("composer-1", "Composer 1"),
    ("grok-code-fast-1", "Grok Code Fast"),
    ("gemini-3-pro", "Gemini 3 Pro"),
    ("gpt-5.1-codex-high", "GPT-5.1 Codex"),
    ("gpt-5", "GPT-5"),
    ("gpt-4.1", "GPT-4.1"),
    ("default", "Auto (Sonnet est.)"),
];

#[cfg(feature = "sqlite")]
const CURSOR_DEFAULT_MODEL: &str = "claude-sonnet-4-5";

pub struct CursorProvider {
    #[cfg(feature = "sqlite")]
    db_path_override: Option<PathBuf>,
}

impl CursorProvider {
    pub fn new(_db_path_override: Option<PathBuf>) -> Self {
        Self {
            #[cfg(feature = "sqlite")]
            db_path_override: _db_path_override,
        }
    }
}

#[cfg(feature = "sqlite")]
fn resolve_model(raw: &str) -> String {
    if raw.is_empty() || raw == "default" {
        CURSOR_DEFAULT_MODEL.to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(feature = "sqlite")]
fn model_for_display(raw: &str) -> String {
    if raw.is_empty() || raw == "default" {
        "default".to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(feature = "sqlite")]
fn get_cursor_db_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Cursor")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
    } else if cfg!(target_os = "windows") {
        home.join("AppData")
            .join("Roaming")
            .join("Cursor")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
    } else {
        home.join(".config")
            .join("Cursor")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
    }
}

impl Provider for CursorProvider {
    fn name(&self) -> &str {
        "cursor"
    }

    fn display_name(&self) -> &str {
        "Cursor"
    }

    fn model_display_name(&self, model: &str) -> String {
        for (key, name) in MODEL_DISPLAY_NAMES {
            if model == *key || model.starts_with(key) {
                return name.to_string();
            }
        }
        model.to_string()
    }

    fn tool_display_name(&self, raw_tool: &str) -> String {
        raw_tool.to_string()
    }

    fn discover_sessions(&self) -> Vec<SessionSource> {
        #[cfg(feature = "sqlite")]
        {
            let db_path = self
                .db_path_override
                .clone()
                .unwrap_or_else(get_cursor_db_path);
            if !db_path.exists() {
                debug!(path = %db_path.display(), "cursor database not found");
                return Vec::new();
            }
            return vec![SessionSource {
                path: db_path.to_string_lossy().to_string(),
                project: "cursor".to_string(),
                provider: "cursor".to_string(),
            }];
        }
        #[cfg(not(feature = "sqlite"))]
        {
            debug!("cursor provider requires sqlite feature");
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
            return parse_cursor_db(&source.path, seen_keys);
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (source, seen_keys);
            Vec::new()
        }
    }
}

#[cfg(feature = "sqlite")]
fn parse_cursor_db(db_path: &str, seen_keys: &mut HashSet<String>) -> Vec<ParsedProviderCall> {
    use rusqlite::Connection;

    let conn = match Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    // Validate schema
    let has_table: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM cursorDiskKV WHERE key LIKE 'bubbleId:%' LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !has_table {
        return Vec::new();
    }

    let lookback_days = 35;
    let time_floor =
        chrono::Utc::now() - chrono::Duration::days(lookback_days);
    let time_floor_str = time_floor.to_rfc3339();

    // Load user messages for conversation context
    let mut user_msg_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT
            json_extract(value, '$.conversationId') as conversation_id,
            json_extract(value, '$.createdAt') as created_at,
            substr(json_extract(value, '$.text'), 1, 500) as text
        FROM cursorDiskKV
        WHERE key LIKE 'bubbleId:%'
            AND json_extract(value, '$.type') = 1
            AND json_extract(value, '$.createdAt') > ?
        ORDER BY json_extract(value, '$.createdAt') ASC",
    ) {
        if let Ok(rows) = stmt.query_map([&time_floor_str], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(2)?,
            ))
        }) {
            for row in rows.flatten() {
                if let (Some(conv_id), Some(text)) = row {
                    if !conv_id.is_empty() && !text.is_empty() {
                        user_msg_map
                            .entry(conv_id)
                            .or_default()
                            .push(text);
                    }
                }
            }
        }
    }

    // Query bubbles with token counts
    let mut results = Vec::new();
    let query = format!(
        "SELECT
            json_extract(value, '$.tokenCount.inputTokens') as input_tokens,
            json_extract(value, '$.tokenCount.outputTokens') as output_tokens,
            json_extract(value, '$.modelInfo.modelName') as model,
            json_extract(value, '$.createdAt') as created_at,
            json_extract(value, '$.conversationId') as conversation_id,
            substr(json_extract(value, '$.text'), 1, 500) as user_text,
            json_extract(value, '$.codeBlocks') as code_blocks
        FROM cursorDiskKV
        WHERE key LIKE 'bubbleId:%'
            AND json_extract(value, '$.tokenCount.inputTokens') > 0
            AND json_extract(value, '$.createdAt') > ?
        ORDER BY json_extract(value, '$.createdAt') ASC"
    );

    let mut stmt = match conn.prepare(&query) {
        Ok(s) => s,
        Err(_) => return results,
    };

    let rows = match stmt.query_map([&time_floor_str], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return results,
    };

    for row in rows.flatten() {
        let (input_tokens, output_tokens, model_raw, created_at, conversation_id, user_text, code_blocks) = row;
        let input = input_tokens.unwrap_or(0) as u64;
        let output = output_tokens.unwrap_or(0) as u64;
        if input == 0 && output == 0 {
            continue;
        }

        let conv_id = conversation_id.unwrap_or_else(|| "unknown".to_string());
        let ts = created_at.unwrap_or_default();
        let dedup_key = format!("cursor:{}:{}:{}:{}", conv_id, ts, input, output);

        if seen_keys.contains(&dedup_key) {
            continue;
        }
        seen_keys.insert(dedup_key.clone());

        let model_str = model_raw.as_deref().unwrap_or("default");
        let pricing_model = resolve_model(model_str);
        let display_model = model_for_display(model_str);

        let cost_usd = models::calculate_cost(&pricing_model, input, output, 0, 0, 0, "standard");

        let conv_messages = user_msg_map.get_mut(&conv_id);
        let user_question = conv_messages
            .and_then(|msgs| if msgs.is_empty() { None } else { Some(msgs.remove(0)) })
            .unwrap_or_default();
        let assistant_text = user_text.unwrap_or_default();
        let user_msg = format!("{} {}", user_question, assistant_text).trim().to_string();

        let languages = extract_languages(code_blocks.as_deref());
        let has_code = !languages.is_empty();
        let mut cursor_tools: Vec<String> = Vec::new();
        if has_code {
            cursor_tools.push("cursor:edit".to_string());
            for lang in &languages {
                cursor_tools.push(format!("lang:{}", lang));
            }
        }

        results.push(ParsedProviderCall {
            model: display_model,
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            web_search_requests: 0,
            cost_usd,
            tools: cursor_tools,
            bash_commands: vec![],
            timestamp: ts,
            deduplication_key: dedup_key,
            user_message: user_msg,
            session_id: conv_id,
        });
    }

    results
}

#[cfg(feature = "sqlite")]
fn extract_languages(code_blocks_json: Option<&str>) -> Vec<String> {
    let json_str = match code_blocks_json {
        Some(s) if !s.is_empty() => s,
        _ => return Vec::new(),
    };

    let blocks: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut langs = std::collections::HashSet::new();
    for block in &blocks {
        if let Some(lang) = block.get("languageId").and_then(|v| v.as_str()) {
            if !lang.is_empty() && lang != "plaintext" {
                langs.insert(lang.to_string());
            }
        }
    }
    langs.into_iter().collect()
}
