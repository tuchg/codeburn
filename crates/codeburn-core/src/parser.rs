use std::collections::{HashMap, HashSet};

use chrono::DateTime;
use dashmap::DashMap;
use rayon::prelude::*;
use tracing::{debug, info};

use crate::classifier;
use crate::config::Config;
use crate::models;
use crate::timing;
use crate::types::*;

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

fn unsanitize_path(dir_name: &str) -> String {
    dir_name.replace('-', "/")
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
            let bash_duration: f64 = sessions.iter().map(|s| s.bash_duration_seconds).sum();

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
                bash_duration_seconds: bash_duration,
            }
        })
        .collect()
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
    let mut bash_duration = 0.0f64;
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
            tokens += call.usage;
            api_calls += 1;
            total_added += call.lines_added;
            total_removed += call.lines_removed;
            bash_duration += call.bash_duration_seconds;

            for path in &call.file_paths {
                all_files.insert(path.clone());
            }

            let model_key = models::short_model_name(&call.model);
            let model_entry = model_map.entry(model_key).or_default();
            model_entry.calls += 1;
            model_entry.cost_usd += call.cost_usd;
            model_entry.tokens += call.usage;

            for tool in extract_core_tools(&call.tools) {
                *tool_map.entry(tool).or_insert(0) += 1;
            }

            for mcp in &call.mcp_tools {
                let server = mcp.split("__").nth(1).unwrap_or(mcp);
                *mcp_map.entry(server.to_string()).or_insert(0) += 1;
            }

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
        bash_duration_seconds: bash_duration,
        model_breakdown,
        tool_breakdown,
        mcp_breakdown,
        bash_breakdown,
        category_breakdown,
    }
}

/// Discover and parse all provider sessions, merging into unified project list.
/// If `provider_filter` is Some, only that provider is queried.
pub fn discover_and_parse(
    date_range: &DateRange,
    provider_filter: Option<&ProviderKind>,
) -> Vec<ProjectSummary> {
    info!(
        start = %date_range.start,
        end = %date_range.end,
        filter = ?provider_filter,
        "starting session discovery"
    );
    let all_projects: DashMap<String, ProjectSummary> = DashMap::new();

    let overrides = Config::load()
        .map(|c| c.pricing)
        .unwrap_or_default();
    let overrides = overrides.as_slice();

    let providers: Vec<Box<dyn crate::providers::types::Provider + Send + Sync>> =
        match provider_filter {
            Some(kind) => crate::providers::get_provider(kind.as_str()).into_iter().collect(),
            None => crate::providers::get_all_providers(),
        };

    debug!(
        providers = providers.iter().map(|p| p.name()).collect::<Vec<_>>().join(", "),
        "parsing providers in parallel"
    );

    providers.par_iter().for_each(|provider| {
        let provider_projects = parse_provider_sessions(provider.as_ref(), date_range, overrides);
        debug!(
            provider = provider.name(),
            projects = provider_projects.len(),
            "provider parsing complete"
        );
        for p in provider_projects {
            merge_into_dashmap(&all_projects, p);
        }
    });

    let mut projects: Vec<ProjectSummary> =
        all_projects.into_iter().map(|(_, v)| v).collect();
    projects.sort_by(|a, b| b.total_cost_usd.partial_cmp(&a.total_cost_usd).unwrap());
    info!(
        total_projects = projects.len(),
        total_cost = projects.iter().map(|p| p.total_cost_usd).sum::<f64>(),
        "discovery complete"
    );
    projects
}

fn merge_into_dashmap(map: &DashMap<String, ProjectSummary>, p: ProjectSummary) {
    match map.entry(p.project.clone()) {
        dashmap::mapref::entry::Entry::Occupied(mut e) => {
            let existing = e.get_mut();
            existing.sessions.extend(p.sessions);
            existing.total_cost_usd += p.total_cost_usd;
            existing.total_api_calls += p.total_api_calls;
            existing.total_files_changed += p.total_files_changed;
            existing.total_lines_added += p.total_lines_added;
            existing.total_lines_removed += p.total_lines_removed;
            existing.total_duration_seconds += p.total_duration_seconds;
            existing.bash_duration_seconds += p.bash_duration_seconds;
        }
        dashmap::mapref::entry::Entry::Vacant(e) => {
            e.insert(p);
        }
    }
}

fn parse_provider_sessions(
    provider: &dyn crate::providers::types::Provider,
    date_range: &DateRange,
    overrides: &[crate::config::PricingOverride],
) -> Vec<ProjectSummary> {
    let sources = provider.discover_sessions();
    debug!(
        provider = provider.name(),
        sessions = sources.len(),
        "discovered provider sessions"
    );
    if sources.is_empty() {
        return Vec::new();
    }

    // Phase 1: parse all session files in parallel, each with a local seen_keys set.
    // Per-file dedup handles intra-file duplicates; global dedup below handles cross-file ones.
    let parsed_by_source: Vec<(&crate::providers::types::SessionSource, Vec<crate::providers::types::ParsedProviderCall>)> =
        sources
            .par_iter()
            .map(|source| {
                let mut local_seen = HashSet::new();
                let calls = provider.parse_session(source, &mut local_seen);
                (source, calls)
            })
            .collect();

    // Phase 2: global dedup + date-filter + map into ParsedApiCall, then group.
    // Sequential, but only HashMap operations - negligible cost.
    let mut global_seen: HashSet<String> = HashSet::new();
    // Group calls by (session_key, user_message) so multiple consecutive assistant
    // responses to the same user message are merged into one turn (enabling retry detection).
    let mut call_groups: HashMap<(String, String), (String, String, Vec<ParsedApiCall>)> =
        HashMap::new();

    for (source, calls) in parsed_by_source {
        for call in calls {
            // Cross-file dedup: skip if this deduplication key was seen in another file.
            if !global_seen.insert(call.deduplication_key.clone()) {
                continue;
            }

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

            let model_display = provider.model_display_name(&call.model);
            let mapped_tools: Vec<String> = call
                .tools
                .iter()
                .map(|t| provider.tool_display_name(t))
                .collect();

            // Re-apply pricing overrides using the raw model name.
            // Use the speed field from the provider call (e.g. "turbo" for Claude fast mode).
            let speed = if call.speed.is_empty() { "standard" } else { &call.speed };
            let cost_usd = if overrides.is_empty() {
                call.cost_usd
            } else {
                models::calculate_cost_with_overrides(
                    &models::CostInput {
                        model: &call.model,
                        input_tokens: call.input_tokens,
                        output_tokens: call.output_tokens,
                        cache_creation_tokens: call.cache_creation_input_tokens,
                        cache_read_tokens: call.cache_read_input_tokens,
                        web_search_requests: call.web_search_requests,
                        speed,
                    },
                    overrides,
                )
            };

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
                cost_usd,
                tools: mapped_tools,
                mcp_tools: extract_mcp_tools(&call.tools),
                bash_commands: call.bash_commands.clone(),
                timestamp: call.timestamp.clone(),
                file_paths: call.file_paths.clone(),
                lines_added: call.lines_added,
                lines_removed: call.lines_removed,
                bash_duration_seconds: call.bash_duration_seconds,
                deduplication_key: call.deduplication_key.clone(),
            };

            let session_key = format!(
                "{}:{}:{}",
                source.provider, call.session_id, source.project
            );
            let group_key = (session_key, call.user_message.clone());
            let ts = call.timestamp.clone();
            let sid = call.session_id.clone();
            let entry = call_groups
                .entry(group_key)
                .or_insert_with(|| (ts, sid, Vec::new()));
            entry.2.push(api_call);
        }
    }

    // Build turns from grouped calls, computing retries across multi-call turns.
    let mut turn_map: HashMap<String, Vec<ParsedTurn>> = HashMap::new();

    for ((session_key, user_message), (timestamp, session_id, calls)) in call_groups {
        let all_tools: Vec<String> = calls.iter().flat_map(|c| c.tools.clone()).collect();
        let category = classifier::classify_turn(&all_tools, &user_message);
        let has_edits = classifier::has_edit_tools(&all_tools);
        let retries = classifier::count_retries(&calls);

        let turn = ParsedTurn {
            user_message,
            calls,
            timestamp,
            session_id,
            category,
            retries,
            has_edits,
        };

        turn_map.entry(session_key).or_default().push(turn);
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
