use std::collections::HashMap;

use crate::types::*;

pub fn build_report(projects: &[ProjectSummary], label: &str) -> Report {
    let mut total_cost = 0.0;
    let mut total_calls: u64 = 0;
    let mut total_sessions: u64 = 0;
    let mut total_tokens = TokenUsage::default();
    let mut total_duration = 0.0;
    let mut total_files: u64 = 0;
    let mut total_added: u64 = 0;
    let mut total_removed: u64 = 0;
    let mut model_map: HashMap<String, ModelStats> = HashMap::new();
    let mut category_map: HashMap<String, CategoryStats> = HashMap::new();
    let mut tool_map: HashMap<String, u64> = HashMap::new();
    let mut mcp_map: HashMap<String, u64> = HashMap::new();
    let mut bash_map: HashMap<String, u64> = HashMap::new();

    for project in projects {
        total_cost += project.total_cost_usd;
        total_calls += project.total_api_calls;
        total_files += project.total_files_changed;
        total_added += project.total_lines_added;
        total_removed += project.total_lines_removed;
        total_duration += project.total_duration_seconds;
        total_sessions += project.sessions.len() as u64;

        for session in &project.sessions {
            total_tokens += session.tokens;

            for (cat, stats) in &session.category_breakdown {
                let entry = category_map.entry(cat.clone()).or_default();
                entry.turns += stats.turns;
                entry.cost_usd += stats.cost_usd;
                entry.duration_seconds += stats.duration_seconds;
                entry.retries += stats.retries;
                entry.edit_turns += stats.edit_turns;
                entry.one_shot_turns += stats.one_shot_turns;
            }

            // Aggregate model/tool/mcp/bash from session breakdowns
            for (model, stats) in &session.model_breakdown {
                let entry = model_map.entry(model.clone()).or_default();
                entry.calls += stats.calls;
                entry.cost_usd += stats.cost_usd;
                entry.tokens += stats.tokens;
            }

            for (tool, count) in &session.tool_breakdown {
                *tool_map.entry(tool.clone()).or_insert(0) += count;
            }

            for (server, count) in &session.mcp_breakdown {
                *mcp_map.entry(server.clone()).or_insert(0) += count;
            }

            for (cmd, count) in &session.bash_breakdown {
                *bash_map.entry(cmd.clone()).or_insert(0) += count;
            }
        }
    }

    let mut model_breakdown: Vec<(String, ModelStats)> = model_map.into_iter().collect();
    model_breakdown.sort_by(|a, b| b.1.cost_usd.partial_cmp(&a.1.cost_usd).unwrap());

    let mut category_breakdown: Vec<(String, CategoryStats)> = category_map.into_iter().collect();
    category_breakdown.sort_by(|a, b| b.1.cost_usd.partial_cmp(&a.1.cost_usd).unwrap());

    let mut tool_breakdown: Vec<(String, u64)> = tool_map.into_iter().collect();
    tool_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    let mut mcp_breakdown: Vec<(String, u64)> = mcp_map.into_iter().collect();
    mcp_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    let mut bash_breakdown: Vec<(String, u64)> = bash_map.into_iter().collect();
    bash_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    let total_input_plus_cache =
        total_tokens.input_tokens + total_tokens.cache_read_tokens;
    let cache_hit_pct = if total_input_plus_cache > 0 {
        (total_tokens.cache_read_tokens as f64 / total_input_plus_cache as f64) * 100.0
    } else {
        0.0
    };

    Report {
        label: label.to_string(),
        total_cost_usd: total_cost,
        total_api_calls: total_calls,
        total_sessions,
        total_tokens,
        total_duration_seconds: total_duration,
        total_files_changed: total_files,
        total_lines_added: total_added,
        total_lines_removed: total_removed,
        cache_hit_pct,
        projects: projects.to_vec(),
        model_breakdown,
        category_breakdown,
        tool_breakdown,
        mcp_breakdown,
        bash_breakdown,
    }
}
