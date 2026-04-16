use colored::Colorize;

use codeburn_core::classifier::category_label;
use codeburn_core::timing::format_duration;
use codeburn_core::types::Report;

fn format_cost(cost: f64) -> String {
    if cost >= 100.0 {
        format!("${:.0}", cost)
    } else if cost >= 1.0 {
        format!("${:.2}", cost)
    } else if cost >= 0.01 {
        format!("${:.3}", cost)
    } else {
        format!("${:.4}", cost)
    }
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

fn bar(value: f64, max: f64, width: usize) -> String {
    if max <= 0.0 {
        return " ".repeat(width);
    }
    let filled = ((value / max) * width as f64).round() as usize;
    let filled = filled.min(width);
    let bar_str: String = "=".repeat(filled);
    let empty: String = " ".repeat(width.saturating_sub(filled));
    format!("{}{}", bar_str.cyan(), empty)
}

pub fn print_report(report: &Report) {
    println!();

    let header = format!("  CODEBURN  {}  ", report.label);
    println!("{}", header.bold().on_bright_black().white());
    println!();

    println!("{}", "  OVERVIEW".bold().yellow());
    println!(
        "  Cost: {}  API calls: {}  Sessions: {}  Cache hit: {}",
        format_cost(report.total_cost_usd).bright_yellow().bold(),
        format!("{}", report.total_api_calls).white().bold(),
        format!("{}", report.total_sessions).white().bold(),
        format!("{:.0}%", report.cache_hit_pct).cyan().bold(),
    );
    println!(
        "  Tokens: {} in / {} out / {} cache-read / {} cache-write",
        format_tokens(report.total_tokens.input_tokens).white(),
        format_tokens(report.total_tokens.output_tokens).white(),
        format_tokens(report.total_tokens.cache_read_tokens).dimmed(),
        format_tokens(report.total_tokens.cache_creation_tokens).dimmed(),
    );
    if report.total_duration_seconds > 0.0 {
        println!(
            "  Duration: {}",
            format_duration(report.total_duration_seconds).cyan().bold(),
        );
    }
    println!();

    println!("{}", "  CODE CHANGES".bold().green());
    println!(
        "  Files changed: {}  Lines added: {}  Lines removed: {}",
        format!("{}", report.total_files_changed).white().bold(),
        format!("+{}", report.total_lines_added).green().bold(),
        format!("-{}", report.total_lines_removed).red().bold(),
    );
    println!();

    if !report.projects.is_empty() {
        println!("{}", "  PROJECTS".bold().green());
        let max_cost = report
            .projects
            .iter()
            .map(|p| p.total_cost_usd)
            .fold(0.0_f64, f64::max);

        for p in report.projects.iter().take(10) {
            let cost_str = format_cost(p.total_cost_usd);
            let bar_str = bar(p.total_cost_usd, max_cost, 20);
            let name = if p.project_path.chars().count() > 30 {
                let byte_offset = p.project_path
                    .char_indices()
                    .rev()
                    .nth(29)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                &p.project_path[byte_offset..]
            } else {
                &p.project_path
            };
            println!(
                "  {:>8}  [{}]  {}  ({} calls, {} sess, {} files, +{}/-{}, {})",
                cost_str.bright_yellow(),
                bar_str,
                name.white(),
                p.total_api_calls,
                p.sessions.len(),
                p.total_files_changed,
                p.total_lines_added,
                p.total_lines_removed,
                format_duration(p.total_duration_seconds).dimmed(),
            );
        }
        println!();
    }

    if !report.model_breakdown.is_empty() {
        println!("{}", "  MODELS".bold().magenta());
        let max_cost = report
            .model_breakdown
            .iter()
            .map(|(_, s)| s.cost_usd)
            .fold(0.0_f64, f64::max);

        for (name, stats) in &report.model_breakdown {
            let cost_str = format_cost(stats.cost_usd);
            let bar_str = bar(stats.cost_usd, max_cost, 20);
            println!(
                "  {:>8}  [{}]  {}  ({} calls)",
                cost_str.bright_yellow(),
                bar_str,
                name.white().bold(),
                stats.calls,
            );
        }
        println!();
    }

    if !report.category_breakdown.is_empty() {
        println!("{}", "  ACTIVITY BREAKDOWN".bold().yellow());
        let max_cost = report
            .category_breakdown
            .iter()
            .map(|(_, s)| s.cost_usd)
            .fold(0.0_f64, f64::max);

        for (name, stats) in &report.category_breakdown {
            let cost_str = format_cost(stats.cost_usd);
            let bar_str = bar(stats.cost_usd, max_cost, 20);
            let duration_str = if stats.duration_seconds > 0.0 {
                format_duration(stats.duration_seconds)
            } else {
                String::new()
            };

            let one_shot_str = if stats.edit_turns > 0 {
                let pct = (stats.one_shot_turns as f64 / stats.edit_turns as f64 * 100.0) as u64;
                format!("{}% 1-shot", pct)
            } else {
                String::new()
            };

            let display_name = category_label(name);

            println!(
                "  {:>8}  [{}]  {:14}  ({} turns{}{})",
                cost_str.bright_yellow(),
                bar_str,
                display_name.white(),
                stats.turns,
                if duration_str.is_empty() {
                    String::new()
                } else {
                    format!(", {}", duration_str)
                },
                if one_shot_str.is_empty() {
                    String::new()
                } else {
                    format!(", {}", one_shot_str)
                },
            );
        }
        println!();
    }

    if !report.tool_breakdown.is_empty() {
        println!("{}", "  TOOLS".bold().cyan());
        let max_calls = report
            .tool_breakdown
            .iter()
            .map(|(_, c)| *c)
            .max()
            .unwrap_or(1);

        for (name, calls) in report.tool_breakdown.iter().take(10) {
            let bar_str = bar(*calls as f64, max_calls as f64, 20);
            println!(
                "  {:>8}  [{}]  {}",
                format!("{}", calls).white().bold(),
                bar_str,
                name.white(),
            );
        }
        println!();
    }

    if !report.bash_breakdown.is_empty() {
        println!("{}", "  SHELL COMMANDS".bold().yellow());
        let max_calls = report
            .bash_breakdown
            .iter()
            .map(|(_, c)| *c)
            .max()
            .unwrap_or(1);

        for (name, calls) in report.bash_breakdown.iter().take(10) {
            let bar_str = bar(*calls as f64, max_calls as f64, 20);
            println!(
                "  {:>8}  [{}]  {}",
                format!("{}", calls).white().bold(),
                bar_str,
                name.white(),
            );
        }
        println!();
    }

    if !report.mcp_breakdown.is_empty() {
        println!("{}", "  MCP SERVERS".bold().magenta());
        let max_calls = report
            .mcp_breakdown
            .iter()
            .map(|(_, c)| *c)
            .max()
            .unwrap_or(1);

        for (name, calls) in report.mcp_breakdown.iter().take(10) {
            let bar_str = bar(*calls as f64, max_calls as f64, 20);
            println!(
                "  {:>8}  [{}]  {}",
                format!("{}", calls).white().bold(),
                bar_str,
                name.white(),
            );
        }
        println!();
    }
}
