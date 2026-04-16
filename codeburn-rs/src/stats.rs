use std::collections::HashMap;

use crate::types::*;

pub fn build_report(projects: &[ProjectSummary], label: &str) -> Report {
    let mut total_cost = 0.0;
    let mut total_calls: u64 = 0;
    let mut total_tokens = TokenUsage::default();
    let mut total_duration = 0.0;
    let mut total_files: u64 = 0;
    let mut total_added: u64 = 0;
    let mut total_removed: u64 = 0;
    let mut model_map: HashMap<String, ModelStats> = HashMap::new();
    let mut category_map: HashMap<String, CategoryStats> = HashMap::new();
    let mut tool_map: HashMap<String, u64> = HashMap::new();

    for project in projects {
        total_cost += project.total_cost_usd;
        total_calls += project.total_api_calls;
        total_files += project.total_files_changed;
        total_added += project.total_lines_added;
        total_removed += project.total_lines_removed;
        total_duration += project.total_duration_seconds;

        for session in &project.sessions {
            total_tokens += session.tokens.clone();

            for (cat, stats) in &session.category_breakdown {
                let entry = category_map.entry(cat.clone()).or_default();
                entry.turns += stats.turns;
                entry.cost_usd += stats.cost_usd;
                entry.duration_seconds += stats.duration_seconds;
            }

            for turn in &session.turns {
                for call in &turn.calls {
                    let model_key = short_model_name(&call.model);
                    let entry = model_map.entry(model_key).or_default();
                    entry.calls += 1;
                    entry.cost_usd += call.cost_usd;
                    entry.tokens += call.usage.clone();

                    for tool in &call.tools {
                        if !tool.starts_with("mcp__") {
                            *tool_map.entry(tool.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }

    let mut model_breakdown: Vec<(String, ModelStats)> = model_map.into_iter().collect();
    model_breakdown.sort_by(|a, b| b.1.cost_usd.partial_cmp(&a.1.cost_usd).unwrap());

    let mut category_breakdown: Vec<(String, CategoryStats)> = category_map.into_iter().collect();
    category_breakdown.sort_by(|a, b| b.1.cost_usd.partial_cmp(&a.1.cost_usd).unwrap());

    let mut tool_breakdown: Vec<(String, u64)> = tool_map.into_iter().collect();
    tool_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    Report {
        label: label.to_string(),
        total_cost_usd: total_cost,
        total_api_calls: total_calls,
        total_tokens,
        total_duration_seconds: total_duration,
        total_files_changed: total_files,
        total_lines_added: total_added,
        total_lines_removed: total_removed,
        projects: projects.to_vec(),
        model_breakdown,
        category_breakdown,
        tool_breakdown,
    }
}

fn short_model_name(model: &str) -> String {
    let canonical = model.split('@').next().unwrap_or(model);

    let names: &[(&str, &str)] = &[
        ("claude-opus-4-6", "Opus 4.6"),
        ("claude-opus-4-5", "Opus 4.5"),
        ("claude-opus-4", "Opus 4"),
        ("claude-sonnet-4-6", "Sonnet 4.6"),
        ("claude-sonnet-4-5", "Sonnet 4.5"),
        ("claude-sonnet-4", "Sonnet 4"),
        ("claude-3-7-sonnet", "Sonnet 3.7"),
        ("claude-3-5-sonnet", "Sonnet 3.5"),
        ("claude-haiku-4-5", "Haiku 4.5"),
        ("claude-3-5-haiku", "Haiku 3.5"),
    ];

    for (prefix, name) in names {
        if canonical.starts_with(prefix) {
            return name.to_string();
        }
    }

    canonical.to_string()
}
