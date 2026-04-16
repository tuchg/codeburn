use std::collections::HashMap;
use std::fs;
use std::path::Path;

use codeburn_core::classifier::category_label;
use codeburn_core::types::*;

fn esc_csv(s: &str) -> String {
    let sanitized = if s.starts_with('=')
        || s.starts_with('+')
        || s.starts_with('-')
        || s.starts_with('@')
    {
        format!("'{}", s)
    } else {
        s.to_string()
    };
    if sanitized.contains(',') || sanitized.contains('"') || sanitized.contains('\n') {
        format!("\"{}\"", sanitized.replace('"', "\"\""))
    } else {
        sanitized
    }
}

fn rows_to_csv(rows: &[Vec<(String, String)>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let headers: Vec<String> = rows[0].iter().map(|(k, _)| k.clone()).collect();
    let mut lines = vec![headers.iter().map(|h| esc_csv(h)).collect::<Vec<_>>().join(",")];
    for row in rows {
        let values: Vec<String> = headers
            .iter()
            .map(|h| {
                row.iter()
                    .find(|(k, _)| k == h)
                    .map(|(_, v)| esc_csv(v))
                    .unwrap_or_default()
            })
            .collect();
        lines.push(values.join(","));
    }
    lines.join("\n")
}

fn build_daily_rows(projects: &[ProjectSummary]) -> Vec<Vec<(String, String)>> {
    let mut daily: HashMap<String, (f64, u64, u64, u64, u64, u64)> = HashMap::new();

    for project in projects {
        for session in &project.sessions {
            for turn in &session.turns {
                if turn.timestamp.is_empty() {
                    continue;
                }
                let day = &turn.timestamp[..10.min(turn.timestamp.len())];
                let entry = daily.entry(day.to_string()).or_default();
                for call in &turn.calls {
                    entry.0 += call.cost_usd;
                    entry.1 += 1;
                    entry.2 += call.usage.input_tokens;
                    entry.3 += call.usage.output_tokens;
                    entry.4 += call.usage.cache_read_tokens;
                    entry.5 += call.usage.cache_creation_tokens;
                }
            }
        }
    }

    let mut days: Vec<_> = daily.into_iter().collect();
    days.sort_by(|a, b| a.0.cmp(&b.0));

    days.iter()
        .map(|(date, d)| {
            vec![
                ("Date".into(), date.clone()),
                ("Cost (USD)".into(), format!("{:.4}", d.0)),
                ("API Calls".into(), d.1.to_string()),
                ("Input Tokens".into(), d.2.to_string()),
                ("Output Tokens".into(), d.3.to_string()),
                ("Cache Read Tokens".into(), d.4.to_string()),
                ("Cache Write Tokens".into(), d.5.to_string()),
            ]
        })
        .collect()
}

fn build_activity_rows(projects: &[ProjectSummary]) -> Vec<Vec<(String, String)>> {
    let mut totals: HashMap<String, (u64, f64)> = HashMap::new();
    for project in projects {
        for session in &project.sessions {
            for (cat, stats) in &session.category_breakdown {
                let entry = totals.entry(cat.clone()).or_default();
                entry.0 += stats.turns;
                entry.1 += stats.cost_usd;
            }
        }
    }

    let mut sorted: Vec<_> = totals.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());

    sorted
        .iter()
        .map(|(cat, d)| {
            vec![
                ("Activity".into(), category_label(cat).to_string()),
                ("Cost (USD)".into(), format!("{:.4}", d.1)),
                ("Turns".into(), d.0.to_string()),
            ]
        })
        .collect()
}

fn build_model_rows(projects: &[ProjectSummary]) -> Vec<Vec<(String, String)>> {
    let mut totals: HashMap<String, (u64, f64, u64, u64)> = HashMap::new();
    for project in projects {
        for session in &project.sessions {
            for (model, stats) in &session.model_breakdown {
                let entry = totals.entry(model.clone()).or_default();
                entry.0 += stats.calls;
                entry.1 += stats.cost_usd;
                entry.2 += stats.tokens.input_tokens;
                entry.3 += stats.tokens.output_tokens;
            }
        }
    }

    let mut sorted: Vec<_> = totals.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());

    sorted
        .iter()
        .map(|(model, d)| {
            vec![
                ("Model".into(), model.clone()),
                ("Cost (USD)".into(), format!("{:.4}", d.1)),
                ("API Calls".into(), d.0.to_string()),
                ("Input Tokens".into(), d.2.to_string()),
                ("Output Tokens".into(), d.3.to_string()),
            ]
        })
        .collect()
}

fn build_tool_rows(projects: &[ProjectSummary]) -> Vec<Vec<(String, String)>> {
    let mut totals: HashMap<String, u64> = HashMap::new();
    for project in projects {
        for session in &project.sessions {
            for (tool, count) in &session.tool_breakdown {
                *totals.entry(tool.clone()).or_insert(0) += count;
            }
        }
    }

    let mut sorted: Vec<_> = totals.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    sorted
        .iter()
        .map(|(tool, calls)| {
            vec![
                ("Tool".into(), tool.clone()),
                ("Calls".into(), calls.to_string()),
            ]
        })
        .collect()
}

fn build_bash_rows(projects: &[ProjectSummary]) -> Vec<Vec<(String, String)>> {
    let mut totals: HashMap<String, u64> = HashMap::new();
    for project in projects {
        for session in &project.sessions {
            for (cmd, count) in &session.bash_breakdown {
                *totals.entry(cmd.clone()).or_insert(0) += count;
            }
        }
    }

    let mut sorted: Vec<_> = totals.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    sorted
        .iter()
        .map(|(cmd, calls)| {
            vec![
                ("Command".into(), cmd.clone()),
                ("Calls".into(), calls.to_string()),
            ]
        })
        .collect()
}

fn build_project_rows(projects: &[ProjectSummary]) -> Vec<Vec<(String, String)>> {
    projects
        .iter()
        .map(|p| {
            vec![
                ("Project".into(), p.project_path.clone()),
                ("Cost (USD)".into(), format!("{:.4}", p.total_cost_usd)),
                ("API Calls".into(), p.total_api_calls.to_string()),
                ("Sessions".into(), p.sessions.len().to_string()),
            ]
        })
        .collect()
}

pub struct PeriodExport<'a> {
    pub label: String,
    pub projects: &'a [ProjectSummary],
}

pub fn export_csv(periods: &[PeriodExport], output_path: &Path) -> Result<String, std::io::Error> {
    let all_projects = periods
        .iter()
        .find(|p| p.label == "30 Days")
        .or_else(|| periods.last())
        .map(|p| p.projects)
        .unwrap_or(&[]);

    let mut parts: Vec<String> = Vec::new();

    // Summary
    parts.push("# Summary".to_string());
    let summary_rows: Vec<Vec<(String, String)>> = periods
        .iter()
        .map(|period| {
            let cost: f64 = period.projects.iter().map(|p| p.total_cost_usd).sum();
            let calls: u64 = period.projects.iter().map(|p| p.total_api_calls).sum();
            let sessions: u64 = period.projects.iter().map(|p| p.sessions.len() as u64).sum();
            vec![
                ("Period".into(), period.label.clone()),
                ("Cost (USD)".into(), format!("{:.4}", cost)),
                ("API Calls".into(), calls.to_string()),
                ("Sessions".into(), sessions.to_string()),
            ]
        })
        .collect();
    parts.push(rows_to_csv(&summary_rows));
    parts.push(String::new());

    for period in periods {
        parts.push(format!("# Daily - {}", period.label));
        parts.push(rows_to_csv(&build_daily_rows(period.projects)));
        parts.push(String::new());

        parts.push(format!("# Activity - {}", period.label));
        parts.push(rows_to_csv(&build_activity_rows(period.projects)));
        parts.push(String::new());

        parts.push(format!("# Models - {}", period.label));
        parts.push(rows_to_csv(&build_model_rows(period.projects)));
        parts.push(String::new());
    }

    parts.push("# Tools - All".to_string());
    parts.push(rows_to_csv(&build_tool_rows(all_projects)));
    parts.push(String::new());

    parts.push("# Shell Commands - All".to_string());
    parts.push(rows_to_csv(&build_bash_rows(all_projects)));
    parts.push(String::new());

    parts.push("# Projects - All".to_string());
    parts.push(rows_to_csv(&build_project_rows(all_projects)));
    parts.push(String::new());

    let full_path = std::path::absolute(output_path)
        .unwrap_or_else(|_| output_path.to_path_buf());
    fs::write(&full_path, parts.join("\n"))?;
    Ok(full_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_injection_prevention() {
        assert_eq!(esc_csv("=SUM(1,2)"), "\"'=SUM(1,2)\"");
        assert_eq!(esc_csv("+danger"), "'+danger");
        assert_eq!(esc_csv("-value"), "'-value");
        assert_eq!(esc_csv("@malicious"), "'@malicious");
    }

    #[test]
    fn test_normal_values_unchanged() {
        assert_eq!(esc_csv("hello"), "hello");
        assert_eq!(esc_csv("42"), "42");
    }

    #[test]
    fn test_commas_quoted() {
        assert_eq!(esc_csv("a,b"), "\"a,b\"");
    }

    #[test]
    fn test_quotes_escaped() {
        assert_eq!(esc_csv("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
