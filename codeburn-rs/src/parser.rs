use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::DateTime;

use crate::timing;
use crate::types::*;

const EDIT_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "FileEditTool",
    "FileWriteTool",
    "NotebookEdit",
];

const READ_TOOLS: &[&str] = &["Read", "Grep", "Glob", "FileReadTool", "GrepTool", "GlobTool"];

const BASH_TOOLS: &[&str] = &["Bash", "BashTool", "PowerShellTool"];

const MODEL_PRICING: &[(&str, f64, f64)] = &[
    ("claude-opus-4", 15.0, 75.0),
    ("claude-sonnet-4", 3.0, 15.0),
    ("claude-haiku-4", 0.80, 4.0),
    ("claude-3-7-sonnet", 3.0, 15.0),
    ("claude-3-5-sonnet", 3.0, 15.0),
    ("claude-3-5-haiku", 0.80, 4.0),
];

fn calculate_cost(model: &str, usage: &TokenUsage) -> f64 {
    let (input_price, output_price) = MODEL_PRICING
        .iter()
        .find(|(prefix, _, _)| model.starts_with(prefix))
        .map(|(_, i, o)| (*i, *o))
        .unwrap_or((3.0, 15.0));

    let cache_write_price = input_price * 1.25;
    let cache_read_price = input_price * 0.1;

    let input_cost = (usage.input_tokens as f64) * input_price / 1_000_000.0;
    let output_cost = (usage.output_tokens as f64) * output_price / 1_000_000.0;
    let cache_write_cost = (usage.cache_creation_tokens as f64) * cache_write_price / 1_000_000.0;
    let cache_read_cost = (usage.cache_read_tokens as f64) * cache_read_price / 1_000_000.0;

    input_cost + output_cost + cache_write_cost + cache_read_cost
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

fn classify_turn(tools: &[String], user_msg: &str) -> String {
    let has_edit = tools.iter().any(|t| EDIT_TOOLS.contains(&t.as_str()));
    let has_read = tools.iter().any(|t| READ_TOOLS.contains(&t.as_str()));
    let has_bash = tools.iter().any(|t| BASH_TOOLS.contains(&t.as_str()));
    let has_agent = tools.iter().any(|t| t == "Agent");
    let has_plan = tools.iter().any(|t| t == "EnterPlanMode");
    let msg_lower = user_msg.to_lowercase();

    if has_plan {
        return "Planning".to_string();
    }
    if has_agent {
        return "Delegation".to_string();
    }

    if has_bash && !has_edit {
        if msg_lower.contains("test")
            || msg_lower.contains("vitest")
            || msg_lower.contains("jest")
            || msg_lower.contains("pytest")
        {
            return "Testing".to_string();
        }
        if msg_lower.contains("git ") {
            return "Git Ops".to_string();
        }
        if msg_lower.contains("build")
            || msg_lower.contains("deploy")
            || msg_lower.contains("docker")
        {
            return "Build/Deploy".to_string();
        }
    }

    if has_edit {
        if msg_lower.contains("fix")
            || msg_lower.contains("bug")
            || msg_lower.contains("error")
            || msg_lower.contains("debug")
        {
            return "Debugging".to_string();
        }
        if msg_lower.contains("refactor")
            || msg_lower.contains("rename")
            || msg_lower.contains("cleanup")
        {
            return "Refactoring".to_string();
        }
        if msg_lower.contains("add")
            || msg_lower.contains("create")
            || msg_lower.contains("implement")
            || msg_lower.contains("feature")
        {
            return "Feature Dev".to_string();
        }
        return "Coding".to_string();
    }

    if has_bash && has_read {
        return "Exploration".to_string();
    }
    if has_bash {
        return "Coding".to_string();
    }
    if has_read {
        return "Exploration".to_string();
    }

    if tools.is_empty() {
        return "Conversation".to_string();
    }

    "General".to_string()
}

fn parse_api_call(entry: &JournalEntry) -> Option<ParsedApiCall> {
    if entry.entry_type != "assistant" {
        return None;
    }

    let msg = entry.message.as_ref()?;
    let model = msg.get("model")?.as_str()?;
    let usage_val = msg.get("usage")?;

    let usage: ApiUsage = serde_json::from_value(usage_val.clone()).ok()?;

    let tokens = TokenUsage {
        input_tokens: usage.input_tokens.unwrap_or(0),
        output_tokens: usage.output_tokens.unwrap_or(0),
        cache_creation_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
        cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
    };

    let content = msg
        .get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

    let tools = extract_tools(&content);
    let file_paths = extract_file_paths(&content);
    let (lines_added, lines_removed) = extract_code_changes(&content);
    let cost_usd = calculate_cost(model, &tokens);

    Some(ParsedApiCall {
        model: model.to_string(),
        usage: tokens,
        cost_usd,
        tools,
        timestamp: entry.timestamp.clone().unwrap_or_default(),
        file_paths,
        lines_added,
        lines_removed,
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
                    let category = classify_turn(&all_tools, &current_user_msg);
                    turns.push(ParsedTurn {
                        user_message: current_user_msg.clone(),
                        calls: std::mem::take(&mut current_calls),
                        timestamp: current_ts.clone(),
                        session_id: current_session_id.clone(),
                        category,
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
        let category = classify_turn(&all_tools, &current_user_msg);
        turns.push(ParsedTurn {
            user_message: current_user_msg,
            calls: current_calls,
            timestamp: current_ts,
            session_id: current_session_id,
            category,
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

    for turn in &turns {
        let turn_cost: f64 = turn.calls.iter().map(|c| c.cost_usd).sum();
        let entry = category_map
            .entry(turn.category.clone())
            .or_default();
        entry.turns += 1;
        entry.cost_usd += turn_cost;

        for call in &turn.calls {
            total_cost += call.cost_usd;
            tokens += call.usage.clone();
            api_calls += 1;
            total_added += call.lines_added;
            total_removed += call.lines_removed;

            for path in &call.file_paths {
                all_files.insert(path.clone());
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

pub fn discover_and_parse(claude_dir: &Path, date_range: &DateRange) -> Vec<ProjectSummary> {
    let projects_dir = claude_dir.join("projects");
    let mut seen_ids = HashSet::new();
    let mut project_map: HashMap<String, Vec<SessionSummary>> = HashMap::new();

    let project_dirs = match fs::read_dir(&projects_dir) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .collect::<Vec<_>>(),
        Err(_) => {
            eprintln!(
                "No Claude sessions found at: {}",
                projects_dir.display()
            );
            return Vec::new();
        }
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

    let mut projects: Vec<ProjectSummary> = project_map
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
        .collect();

    projects.sort_by(|a, b| b.total_cost_usd.partial_cmp(&a.total_cost_usd).unwrap());
    projects
}
