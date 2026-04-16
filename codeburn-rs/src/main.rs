mod bash_utils;
mod classifier;
mod config;
mod display;
mod export;
mod models;
mod parser;
mod providers;
mod stats;
mod timing;
mod tui;
mod types;

use std::io::IsTerminal;

use chrono::NaiveDate;
use clap::{Parser, Subcommand};

use crate::parser::discover_and_parse;
use crate::types::{DateSpec, Period, ProviderKind};

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
    /// Show interactive TUI dashboard (default when TTY)
    Dashboard {
        /// Period to display initially
        #[arg(short, long, default_value = "week", value_enum)]
        period: Period,
        /// Filter to a specific provider
        #[arg(long, value_enum)]
        provider: Option<ProviderKind>,
    },
    /// Show plain-text usage report (always non-interactive)
    Report {
        /// Period: week (default), today, 30days, month, all
        #[arg(short, long, default_value = "week", value_enum)]
        period: Period,
        /// Filter to a specific provider
        #[arg(long, value_enum)]
        provider: Option<ProviderKind>,
        /// Custom start date (YYYY-MM-DD), overrides --period
        #[arg(long)]
        since: Option<NaiveDate>,
        /// Custom end date (YYYY-MM-DD), used with --since
        #[arg(long)]
        until: Option<NaiveDate>,
    },
    /// Show today's usage (plain text)
    Today {
        /// Filter to a specific provider
        #[arg(long, value_enum)]
        provider: Option<ProviderKind>,
    },
    /// Show this month's usage (plain text)
    Month {
        /// Filter to a specific provider
        #[arg(long, value_enum)]
        provider: Option<ProviderKind>,
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

/// Build a DateSpec from the report command's arguments.
fn resolve_date_spec(period: Period, since: &Option<NaiveDate>, until: &Option<NaiveDate>) -> DateSpec {
    if let Some(start) = since {
        let end = until.unwrap_or_else(|| {
            chrono::Local::now().date_naive().succ_opt().unwrap_or(chrono::Local::now().date_naive())
        });
        DateSpec::Custom {
            start: *start,
            end,
            label: None,
        }
    } else {
        DateSpec::Period(period)
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
            let periods_config: &[(Period, &str)] = &[
                (Period::Today, "Today"),
                (Period::Week, "7 Days"),
                (Period::Days30, "30 Days"),
                (Period::Month, "This Month"),
            ];
            let mut period_data: Vec<(String, Vec<types::ProjectSummary>)> = Vec::new();
            for (period, label) in periods_config {
                let (date_range, _) = period.date_range();
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
        Some(Commands::Dashboard { period, provider }) => {
            if let Err(e) = tui::run_tui(*period, provider.as_ref()) {
                eprintln!("TUI error: {}", e);
            }
        }
        _ => {
            let (date_spec, provider_filter) = match &cli.command {
                Some(Commands::Report { period, provider, since, until }) => {
                    (resolve_date_spec(*period, since, until), provider.as_ref())
                }
                Some(Commands::Today { provider }) => (DateSpec::Period(Period::Today), provider.as_ref()),
                Some(Commands::Month { provider }) => (DateSpec::Period(Period::Month), provider.as_ref()),
                None => (DateSpec::Period(Period::Week), None),
                _ => unreachable!(),
            };

            // Launch TUI when running interactively; fall back to plain text when piped.
            if std::io::stdout().is_terminal() && cli.command.is_none() {
                if let Err(e) = tui::run_tui(Period::Week, provider_filter) {
                    eprintln!("TUI error: {}", e);
                }
                return;
            }

            let (date_range, label) = date_spec.date_range();
            let projects = discover_and_parse(&date_range, provider_filter);
            let report = stats::build_report(&projects, &label);
            display::print_report(&report);
        }
    }
}
