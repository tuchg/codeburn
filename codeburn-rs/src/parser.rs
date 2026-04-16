use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::DateTime;
use rayon::prelude::*;

use crate::bash_utils::{extract_bash_commands, is_bash_tool};
use crate::classifier;
use crate::models;
use crate::timing;
use crate::types::*;

const EDIT_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "FileEditTool",
    "FileWriteTool",
    "NotebookEdit",
    "cursor:edit",
];

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

fn extract_mcp_tools(tools: &[String]) -> Vec<String> {
    tools
        .iter()
        .filter(|t| t.starts_with("mcp__"))
        .cloned()
        .collect()
}

fn extract_core_tools(tools: &[String]) -> Vec<String> {
    tools
        .iter()
        .filter(|t| !t.starts_with("mcp__"))
        .cloned()
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
        let tool_type = block.get("type").and_then(|v| v.as_str());
        if tool_type != Some("tool_use") {
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

fn parse_api_call(entry: &JournalEntry) -> Option<ParsedApiCall> {
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

    let tokens = TokenUsage {
        input_tokens: usage.input_tokens.unwrap_or(0),
        output_tokens: usage.output_tokens.unwrap_or(0),
        cache_creation_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
        cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
        cached_tokens: 0,
        reasoning_tokens: 0,
        web_search_requests,
    };

    let content = msg
        .get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

    let tools = extract_tools(&content);
    let mcp_tools = extract_mcp_tools(&tools);
    let bash_commands = extract_bash_commands_from_content(&content);
    let file_paths = extract_file_paths(&content);
    let (lines_added, lines_removed) = extract_code_changes(&content);

    let cost_usd = models::calculate_cost(
        model,
        tokens.input_tokens,
        tokens.output_tokens,
        tokens.cache_creation_tokens,
        tokens.cache_read_tokens,
        web_search_requests,
        speed,
    );

    let dedup_key = msg
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("claude:{}", entry.timestamp.as_deref().unwrap_or("")));

    Some(ParsedApiCall {
        provider: "claude".to_string(),
        model: model.to_string(),
        usage: tokens,
        cost_usd,
        tools,
        mcp_tools,
        bash_commands,
        timestamp: entry.timestamp.clone().unwrap_or_default(),
        file_paths,
        lines_added,
        lines_removed,
        deduplication_key: dedup_key,
    })
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

fn group_into_turns(entries: &[JournalEntry], seen_ids: &mut HashSet<String>) -> Vec<ParsedTurn> {
    let mut turns = Vec::new();
    let mut current_user_msg = String::new();
    let mut current_calls: Vec<ParsedApiCall> = Vec::new();
    let mut current_ts = String::new();
    let mut current_session_id = String::new();

    for entry in entries {
        if entry.entry_type == "user" {
            let text = get_user_message(entry);
            if !text.trim().is_empty() {
                if !current_calls.is_empty() {
                    let all_tools: Vec<String> =
                        current_calls.iter().flat_map(|c| c.tools.clone()).collect();
                    let category = classifier::classify_turn(&all_tools, &current_user_msg);
                    let retries = classifier::count_retries(&current_calls);
                    let has_edits = classifier::has_edit_tools(&all_tools);
                    turns.push(ParsedTurn {
                        user_message: current_user_msg.clone(),
                        calls: std::mem::take(&mut current_calls),
                        timestamp: current_ts.clone(),
                        session_id: current_session_id.clone(),
                        category,
                        retries,
                        has_edits,
                    });
                }
                current_user_msg = text;
                current_calls = Vec::new();
                current_ts = entry.timestamp.clone().unwrap_or_default();
                current_session_id = entry.session_id.clone().unwrap_or_default();
            }
        } else if entry.entry_type == "assistant" {
            if let Some(msg_id) = get_message_id(entry) {
                if seen_ids.contains(&msg_id) {
                    continue;
                }
                seen_ids.insert(msg_id);
            }
            if let Some(call) = parse_api_call(entry) {
                current_calls.push(call);
            }
        }
    }

    if !current_calls.is_empty() {
        let all_tools: Vec<String> = current_calls.iter().flat_map(|c| c.tools.clone()).collect();
        let category = classifier::classify_turn(&all_tools, &current_user_msg);
        let retries = classifier::count_retries(&current_calls);
        let has_edits = classifier::has_edit_tools(&all_tools);
        turns.push(ParsedTurn {
            user_message: current_user_msg,
            calls: current_calls,
            timestamp: current_ts,
            session_id: current_session_id,
            category,
            retries,
            has_edits,
        });
    }

    turns
}

fn build_session_summary(
    session_id: &str,
    project: &str,
    turns: Vec<ParsedTurn>,
) -> SessionSummary {
    let mut total_cost = 0.0;
    let mut tokens = TokenUsage::default();
    let mut api_calls: u64 = 0;
    let mut all_files: HashSet<String> = HashSet::new();
    let mut total_added: u64 = 0;
    let mut total_removed: u64 = 0;
    let mut first_ts = String::new();
    let mut last_ts = String::new();
    let mut category_map: HashMap<String, CategoryStats> = HashMap::new();
    let mut model_map: HashMap<String, ModelStats> = HashMap::new();
    let mut tool_map: HashMap<String, u64> = HashMap::new();
    let mut mcp_map: HashMap<String, u64> = HashMap::new();
    let mut bash_map: HashMap<String, u64> = HashMap::new();

    for turn in &turns {
        let turn_cost: f64 = turn.calls.iter().map(|c| c.cost_usd).sum();
        let entry = category_map.entry(turn.category.clone()).or_default();
        entry.turns += 1;
        entry.cost_usd += turn_cost;
        if turn.has_edits {
            entry.edit_turns += 1;
            entry.retries += turn.retries;
            if turn.retries == 0 {
                entry.one_shot_turns += 1;
            }
        }

        for call in &turn.calls {
            total_cost += call.cost_usd;
            tokens += call.usage.clone();
            api_calls += 1;
            total_added += call.lines_added;
            total_removed += call.lines_removed;

            for path in &call.file_paths {
                all_files.insert(path.clone());
            }

            // Model breakdown
            let model_key = models::short_model_name(&call.model);
            let model_entry = model_map.entry(model_key).or_default();
            model_entry.calls += 1;
            model_entry.cost_usd += call.cost_usd;
            model_entry.tokens += call.usage.clone();

            // Core tools
            for tool in extract_core_tools(&call.tools) {
                *tool_map.entry(tool).or_insert(0) += 1;
            }

            // MCP tools
            for mcp in &call.mcp_tools {
                let server = mcp.split("__").nth(1).unwrap_or(mcp);
                *mcp_map.entry(server.to_string()).or_insert(0) += 1;
            }

            // Bash commands
            for cmd in &call.bash_commands {
                *bash_map.entry(cmd.clone()).or_insert(0) += 1;
            }

            if first_ts.is_empty() || (!call.timestamp.is_empty() && call.timestamp < first_ts) {
                first_ts = call.timestamp.clone();
            }
            if last_ts.is_empty() || call.timestamp > last_ts {
                last_ts = call.timestamp.clone();
            }
        }
    }

    let duration_seconds = timing::calculate_duration(&first_ts, &last_ts);

    timing::assign_turn_durations(&turns, &mut category_map);

    let mut category_breakdown: Vec<(String, CategoryStats)> = category_map.into_iter().collect();
    category_breakdown.sort_by(|a, b| b.1.cost_usd.partial_cmp(&a.1.cost_usd).unwrap());

    let mut model_breakdown: Vec<(String, ModelStats)> = model_map.into_iter().collect();
    model_breakdown.sort_by(|a, b| b.1.cost_usd.partial_cmp(&a.1.cost_usd).unwrap());

    let mut tool_breakdown: Vec<(String, u64)> = tool_map.into_iter().collect();
    tool_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    let mut mcp_breakdown: Vec<(String, u64)> = mcp_map.into_iter().collect();
    mcp_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    let mut bash_breakdown: Vec<(String, u64)> = bash_map.into_iter().collect();
    bash_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    SessionSummary {
        session_id: session_id.to_string(),
        project: project.to_string(),
        first_timestamp: first_ts,
        last_timestamp: last_ts,
        total_cost_usd: total_cost,
        tokens,
        api_calls,
        turns,
        files_changed: all_files.into_iter().collect(),
        total_lines_added: total_added,
        total_lines_removed: total_removed,
        duration_seconds,
        model_breakdown,
        tool_breakdown,
        mcp_breakdown,
        bash_breakdown,
        category_breakdown,
    }
}

fn parse_session_file(
    file_path: &Path,
    project: &str,
    seen_ids: &mut HashSet<String>,
    date_range: &DateRange,
) -> Option<SessionSummary> {
    let content = fs::read_to_string(file_path).ok()?;
    let entries: Vec<JournalEntry> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if entries.is_empty() {
        return None;
    }

    let filtered: Vec<JournalEntry> = entries
        .into_iter()
        .filter(|e| {
            if let Some(ts_str) = &e.timestamp {
                if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                    let date = ts.date_naive();
                    return date >= date_range.start && date < date_range.end;
                }
                if let Ok(ts) = ts_str.parse::<DateTime<chrono::Utc>>() {
                    let date = ts.date_naive();
                    return date >= date_range.start && date < date_range.end;
                }
            }
            e.entry_type == "user"
        })
        .collect();

    if filtered.is_empty() {
        return None;
    }

    let session_id = file_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let turns = group_into_turns(&filtered, seen_ids);
    let summary = build_session_summary(&session_id, project, turns);

    if summary.api_calls > 0 {
        Some(summary)
    } else {
        None
    }
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

fn unsanitize_path(dir_name: &str) -> String {
    dir_name.replace('-', "/")
}

/// Discover and parse Claude Code sessions
fn discover_claude_sessions(claude_dir: &Path, date_range: &DateRange) -> Vec<ProjectSummary> {
    let projects_dir = claude_dir.join("projects");
    let mut seen_ids = HashSet::new();
    let mut project_map: HashMap<String, Vec<SessionSummary>> = HashMap::new();

    let project_dirs = match fs::read_dir(&projects_dir) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .collect::<Vec<_>>(),
        Err(_) => return Vec::new(),
    };

    for dir_entry in project_dirs {
        let dir_path = dir_entry.path();
        let dir_name = dir_entry.file_name().to_string_lossy().to_string();

        let jsonl_files = collect_jsonl_files(&dir_path);
        for file_path in jsonl_files {
            if let Some(session) =
                parse_session_file(&file_path, &dir_name, &mut seen_ids, date_range)
            {
                project_map
                    .entry(dir_name.clone())
                    .or_default()
                    .push(session);
            }
        }
    }

    project_map_to_summaries(project_map)
}

fn project_map_to_summaries(
    project_map: HashMap<String, Vec<SessionSummary>>,
) -> Vec<ProjectSummary> {
    project_map
        .into_iter()
        .map(|(name, sessions)| {
            let total_cost: f64 = sessions.iter().map(|s| s.total_cost_usd).sum();
            let total_calls: u64 = sessions.iter().map(|s| s.api_calls).sum();
            let total_files: u64 = sessions
                .iter()
                .map(|s| s.files_changed.len() as u64)
                .sum();
            let total_added: u64 = sessions.iter().map(|s| s.total_lines_added).sum();
            let total_removed: u64 = sessions.iter().map(|s| s.total_lines_removed).sum();
            let total_duration: f64 = sessions.iter().map(|s| s.duration_seconds).sum();

            ProjectSummary {
                project_path: unsanitize_path(&name),
                project: name,
                sessions,
                total_cost_usd: total_cost,
                total_api_calls: total_calls,
                total_files_changed: total_files,
                total_lines_added: total_added,
                total_lines_removed: total_removed,
                total_duration_seconds: total_duration,
            }
        })
        .collect()
}

fn get_claude_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .map(|h| h.join(".claude"))
        .unwrap_or_else(|| PathBuf::from(".claude"))
}

/// Discover and parse all provider sessions, merging into unified project list.
/// If `provider_filter` is Some, only that provider is queried.
pub fn discover_and_parse(
    date_range: &DateRange,
    provider_filter: Option<&ProviderKind>,
) -> Vec<ProjectSummary> {
    let claude_dir = get_claude_dir();
    let all_projects: Mutex<HashMap<String, ProjectSummary>> = Mutex::new(HashMap::new());

    let include_claude = provider_filter.is_none()
        || provider_filter == Some(&ProviderKind::Claude);

    // Claude is parsed specially (full turn grouping, file change tracking)
    if include_claude {
        for p in discover_claude_sessions(&claude_dir, date_range) {
            merge_project(&mut all_projects.lock().unwrap(), p);
        }
    }

    let providers: Vec<Box<dyn crate::providers::types::Provider + Send + Sync>> =
        match provider_filter {
            Some(kind) if *kind != ProviderKind::Claude => {
                crate::providers::get_provider(kind.as_str()).into_iter().collect()
            }
            None => crate::providers::get_all_providers(),
            _ => Vec::new(),
        };

    // Parse each non-Claude provider in parallel
    providers.par_iter().for_each(|provider| {
        let summaries = parse_provider_sessions(provider.as_ref(), date_range);
        let mut map = all_projects.lock().unwrap();
        for p in summaries {
            merge_project(&mut map, p);
        }
    });

    let mut projects: Vec<ProjectSummary> = all_projects
        .into_inner()
        .unwrap()
        .into_values()
        .collect();
    projects.sort_by(|a, b| b.total_cost_usd.partial_cmp(&a.total_cost_usd).unwrap());
    projects
}

fn merge_project(
    all_projects: &mut HashMap<String, ProjectSummary>,
    p: ProjectSummary,
) {
    let existing = all_projects.entry(p.project.clone()).or_insert_with(|| {
        ProjectSummary {
            project: p.project.clone(),
            project_path: p.project_path.clone(),
            sessions: vec![],
            total_cost_usd: 0.0,
            total_api_calls: 0,
            total_files_changed: 0,
            total_lines_added: 0,
            total_lines_removed: 0,
            total_duration_seconds: 0.0,
        }
    });
    existing.sessions.extend(p.sessions);
    existing.total_cost_usd += p.total_cost_usd;
    existing.total_api_calls += p.total_api_calls;
    existing.total_files_changed += p.total_files_changed;
    existing.total_lines_added += p.total_lines_added;
    existing.total_lines_removed += p.total_lines_removed;
    existing.total_duration_seconds += p.total_duration_seconds;
}

/// Parse sessions from any provider using the Provider trait
fn parse_provider_sessions(
    provider: &dyn crate::providers::types::Provider,
    date_range: &DateRange,
) -> Vec<ProjectSummary> {
    let sources = provider.discover_sessions();
    if sources.is_empty() {
        return Vec::new();
    }

    let mut seen_keys = HashSet::new();
    let mut turn_map: HashMap<String, Vec<ParsedTurn>> = HashMap::new();

    for source in &sources {
        let calls = provider.parse_session(source, &mut seen_keys);

        for call in calls {
            // Date filtering
            if !call.timestamp.is_empty() {
                if let Ok(ts) = DateTime::parse_from_rfc3339(&call.timestamp) {
                    let date = ts.date_naive();
                    if date < date_range.start || date >= date_range.end {
                        continue;
                    }
                } else if let Ok(ts) = call.timestamp.parse::<DateTime<chrono::Utc>>() {
                    let date = ts.date_naive();
                    if date < date_range.start || date >= date_range.end {
                        continue;
                    }
                }
            }

            // Convert to ParsedApiCall, applying provider-specific display name mappings
            let model_display = provider.model_display_name(&call.model);
            let mapped_tools: Vec<String> = call
                .tools
                .iter()
                .map(|t| provider.tool_display_name(t))
                .collect();
            let api_call = ParsedApiCall {
                provider: provider.name().to_string(),
                model: model_display,
                usage: TokenUsage {
                    input_tokens: call.input_tokens,
                    output_tokens: call.output_tokens,
                    cache_creation_tokens: call.cache_creation_input_tokens,
                    cache_read_tokens: call.cache_read_input_tokens,
                    cached_tokens: call.cached_input_tokens,
                    reasoning_tokens: call.reasoning_tokens,
                    web_search_requests: call.web_search_requests,
                },
                cost_usd: call.cost_usd,
                tools: mapped_tools,
                mcp_tools: extract_mcp_tools(&call.tools),
                bash_commands: call.bash_commands.clone(),
                timestamp: call.timestamp.clone(),
                file_paths: vec![],
                lines_added: 0,
                lines_removed: 0,
                deduplication_key: call.deduplication_key.clone(),
            };

            let all_tools = api_call.tools.clone();
            let category = classifier::classify_turn(&all_tools, &call.user_message);
            let has_edits = classifier::has_edit_tools(&all_tools);

            let turn = ParsedTurn {
                user_message: call.user_message.clone(),
                calls: vec![api_call],
                timestamp: call.timestamp.clone(),
                session_id: call.session_id.clone(),
                category,
                retries: 0,
                has_edits,
            };

            let key = format!(
                "{}:{}:{}",
                source.provider, call.session_id, source.project
            );

            turn_map
                .entry(key)
                .or_default()
                .push(turn);
        }
    }

    // Build session summaries from collected turns
    let mut result_map: HashMap<String, Vec<SessionSummary>> = HashMap::new();

    for (key, turns) in turn_map {
        let parts: Vec<&str> = key.splitn(3, ':').collect();
        let session_id = parts.get(1).copied().unwrap_or(&key);
        let project_name = parts.get(2).copied().unwrap_or(&key);

        let summary = build_session_summary(session_id, project_name, turns);
        if summary.api_calls > 0 {
            result_map
                .entry(project_name.to_string())
                .or_default()
                .push(summary);
        }
    }

    project_map_to_summaries(result_map)
}
