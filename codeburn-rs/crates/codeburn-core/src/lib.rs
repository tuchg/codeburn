pub(crate) mod bash_utils;
pub mod classifier;
pub mod config;
pub mod models;
pub mod parser;
pub mod providers;
pub mod stats;
pub mod timing;
pub mod types;

pub use types::{
    CategoryStats, DateRange, DateSpec, ModelStats, ParsedApiCall, ParsedTurn, Period,
    ProjectSummary, ProviderKind, Report, SessionSummary, TokenUsage,
};
pub use classifier::category_label;
pub use timing::format_duration;

/// Unified error type for codeburn operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Export error: {0}")]
    Export(String),
    #[error("Config error: {0}")]
    Config(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// High-level entry point: discover sessions, parse them, and build a report.
///
/// This is the primary API for consumers of `codeburn-core`. It takes a date
/// specification and an optional provider filter, discovers all matching
/// sessions, and returns a fully aggregated `Report`.
///
/// # Examples
///
/// ```no_run
/// use codeburn_core::{analyze, Period, DateSpec};
///
/// let report = analyze(DateSpec::Period(Period::Week), None);
/// println!("Total cost: ${:.2}", report.total_cost_usd);
/// ```
pub fn analyze(date_spec: DateSpec, provider_filter: Option<&ProviderKind>) -> Report {
    let (date_range, label) = date_spec.date_range();
    let projects = parser::discover_and_parse(&date_range, provider_filter);
    stats::build_report(&projects, &label)
}
