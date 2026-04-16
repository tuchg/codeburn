mod bash_utils;
mod classifier;
mod display;
mod export;
mod models;
mod parser;
mod providers;
mod stats;
mod timing;
mod types;

use chrono::{Datelike, Local, NaiveDate};
use clap::{Parser, Subcommand};

use crate::parser::discover_and_parse;
use crate::types::DateRange;

#[derive(Parser)]
#[command(
    name = "codeburn",
    about = "See where your AI coding tokens go -- with timing, file changes, and code diff stats"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show usage report for a period (default: last 7 days)
    Report {
        /// Period: today, week, 30days, month, all
        #[arg(short, long, default_value = "week")]
        period: String,
        /// Filter to a specific provider (e.g. claude, codex, cursor, gemini, copilot, pi)
        #[arg(long)]
        provider: Option<String>,
    },
    /// Show today's usage
    Today {
        /// Filter to a specific provider
        #[arg(long)]
        provider: Option<String>,
    },
    /// Show this month's usage
    Month {
        /// Filter to a specific provider
        #[arg(long)]
        provider: Option<String>,
    },
    /// Export usage data to CSV
    Export {
        /// Output file path
        #[arg(short, long, default_value = "codeburn-report.csv")]
        output: String,
    },
    /// List all supported providers
    Providers,
}

fn get_date_range(period: &str) -> (DateRange, String) {
    let today = Local::now().date_naive();
    let end = today.succ_opt().unwrap_or(today);

    match period {
        "today" => {
            let label = format!("Today ({})", today);
            (DateRange { start: today, end }, label)
        }
        "week" => {
            let start = today - chrono::Days::new(7);
            (DateRange { start, end }, "Last 7 Days".to_string())
        }
        "month" => {
            let start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
            let label = format!("{} {}", month_name(today.month()), today.year());
            (DateRange { start, end }, label)
        }
        "30days" => {
            let start = today - chrono::Days::new(30);
            (DateRange { start, end }, "Last 30 Days".to_string())
        }
        "all" => {
            let start = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap_or(today);
            (DateRange { start, end }, "All Time".to_string())
        }
        _ => {
            let start = today - chrono::Days::new(7);
            (DateRange { start, end }, "Last 7 Days".to_string())
        }
    }
}

fn month_name(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Providers) => {
            let providers = providers::get_all_providers();
            println!("Supported providers:");
            for p in &providers {
                println!("  {} ({})", p.display_name(), p.name());
            }
        }
        Some(Commands::Export { output }) => {
            let periods_config: Vec<(&str, &str)> = vec![
                ("today", "Today"),
                ("week", "7 Days"),
                ("30days", "30 Days"),
                ("month", "This Month"),
            ];

            let mut period_data: Vec<(String, Vec<types::ProjectSummary>)> = Vec::new();
            for (period, label) in &periods_config {
                let (date_range, _) = get_date_range(period);
                let projects = discover_and_parse(&date_range, None);
                period_data.push((label.to_string(), projects));
            }

            let exports: Vec<export::PeriodExport> = period_data
                .iter()
                .map(|(label, projects)| export::PeriodExport {
                    label: label.clone(),
                    projects,
                })
                .collect();

            let output_path = std::path::Path::new(output);
            match export::export_csv(&exports, output_path) {
                Ok(path) => println!("Exported to {}", path),
                Err(e) => eprintln!("Export failed: {}", e),
            }
        }
        _ => {
            let (period, provider_filter, _label) = match &cli.command {
                Some(Commands::Report { period, provider }) => {
                    (period.as_str(), provider.as_deref(), None)
                }
                Some(Commands::Today { provider }) => ("today", provider.as_deref(), None),
                Some(Commands::Month { provider }) => ("month", provider.as_deref(), None),
                None => ("week", None, None),
                Some(Commands::Export { .. }) | Some(Commands::Providers) => unreachable!(),
            };

            let (date_range, period_label) = get_date_range(period);
            let label = _label.unwrap_or(period_label);

            let projects = discover_and_parse(&date_range, provider_filter);
            let report = stats::build_report(&projects, &label);
            display::print_report(&report);
        }
    }
}
