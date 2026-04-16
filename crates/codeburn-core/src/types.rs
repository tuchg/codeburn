use chrono::{Datelike, Local, NaiveDate};
use clap::ValueEnum;

/// Represents a reporting period for the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum Period {
    /// Last 7 days
    #[default]
    Week,
    /// Today only
    Today,
    /// Last 30 days
    #[value(name = "30days")]
    Days30,
    /// Current calendar month
    Month,
    /// All time
    All,
}

impl Period {
    /// Compute the concrete date range and a human-readable label for this period.
    pub fn date_range(&self) -> (DateRange, String) {
        let today = Local::now().date_naive();
        let end = today.succ_opt().unwrap_or(today);
        match self {
            Period::Today => (
                DateRange { start: today, end },
                format!("Today ({})", today),
            ),
            Period::Week => (
                DateRange { start: today - chrono::Days::new(7), end },
                "Last 7 Days".to_string(),
            ),
            Period::Month => {
                let start =
                    NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
                (
                    DateRange { start, end },
                    format!("{} {}", Self::month_name(today.month()), today.year()),
                )
            }
            Period::Days30 => (
                DateRange { start: today - chrono::Days::new(30), end },
                "Last 30 Days".to_string(),
            ),
            Period::All => (
                DateRange {
                    start: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap_or(today),
                    end,
                },
                "All Time".to_string(),
            ),
        }
    }

    /// Short tab label shown in the TUI.
    pub fn label(self) -> &'static str {
        match self {
            Period::Today => "Today",
            Period::Week => "7 Days",
            Period::Days30 => "30 Days",
            Period::Month => "This Month",
            Period::All => "All Time",
        }
    }

    /// Cycle to the previous period (wraps around, skips All).
    pub fn prev(self) -> Self {
        match self {
            Period::Today => Period::Month,
            Period::Week => Period::Today,
            Period::Days30 => Period::Week,
            Period::Month => Period::Days30,
            Period::All => Period::Month,
        }
    }

    /// Cycle to the next period (wraps around, skips All).
    pub fn next(self) -> Self {
        match self {
            Period::Today => Period::Week,
            Period::Week => Period::Days30,
            Period::Days30 => Period::Month,
            Period::Month => Period::Today,
            Period::All => Period::Today,
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
}

/// Known AI coding providers. "Unknown" carries the raw string for pass-through.
#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
#[non_exhaustive]
pub enum ProviderKind {
    Claude,
    Codex,
    Cursor,
    Opencode,
    Gemini,
    Copilot,
    Pi,
}

impl ProviderKind {
    /// Returns the canonical lowercase identifier used in provider registries.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Claude => "claude",
            ProviderKind::Codex => "codex",
            ProviderKind::Cursor => "cursor",
            ProviderKind::Opencode => "opencode",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Copilot => "copilot",
            ProviderKind::Pi => "pi",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// Flexible date specification: either a named period or an arbitrary date range.
/// This is the primary API for library consumers to specify time ranges.
#[derive(Debug, Clone)]
pub enum DateSpec {
    /// One of the built-in named periods.
    Period(Period),
    /// A custom date range with an optional label.
    Custom {
        start: NaiveDate,
        end: NaiveDate,
        label: Option<String>,
    },
}

impl DateSpec {
    /// Compute the concrete date range and a human-readable label.
    pub fn date_range(&self) -> (DateRange, String) {
        match self {
            DateSpec::Period(p) => p.date_range(),
            DateSpec::Custom { start, end, label } => {
                let range_label = label.clone().unwrap_or_else(|| {
                    format!("{} to {}", start, end)
                });
                (DateRange { start: *start, end: *end }, range_label)
            }
        }
    }
}

impl From<Period> for DateSpec {
    fn from(p: Period) -> Self {
        DateSpec::Period(p)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
    pub web_search_requests: u64,
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, rhs: Self) {
        self.input_tokens += rhs.input_tokens;
        self.output_tokens += rhs.output_tokens;
        self.cache_creation_tokens += rhs.cache_creation_tokens;
        self.cache_read_tokens += rhs.cache_read_tokens;
        self.cached_tokens += rhs.cached_tokens;
        self.reasoning_tokens += rhs.reasoning_tokens;
        self.web_search_requests += rhs.web_search_requests;
    }
}

#[derive(Debug, Clone)]
pub struct ParsedApiCall {
    pub provider: String,
    pub model: String,
    pub usage: TokenUsage,
    pub cost_usd: f64,
    pub tools: Vec<String>,
    pub mcp_tools: Vec<String>,
    pub bash_commands: Vec<String>,
    pub timestamp: String,
    pub file_paths: Vec<String>,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub bash_duration_seconds: f64,
    pub deduplication_key: String,
}

#[derive(Debug, Clone)]
pub struct ParsedTurn {
    pub user_message: String,
    pub calls: Vec<ParsedApiCall>,
    pub timestamp: String,
    pub session_id: String,
    pub category: String,
    pub retries: u64,
    pub has_edits: bool,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub project: String,
    pub first_timestamp: String,
    pub last_timestamp: String,
    pub total_cost_usd: f64,
    pub tokens: TokenUsage,
    pub api_calls: u64,
    pub turns: Vec<ParsedTurn>,
    pub files_changed: Vec<String>,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub duration_seconds: f64,
    pub bash_duration_seconds: f64,
    pub model_breakdown: Vec<(String, ModelStats)>,
    pub tool_breakdown: Vec<(String, u64)>,
    pub mcp_breakdown: Vec<(String, u64)>,
    pub bash_breakdown: Vec<(String, u64)>,
    pub category_breakdown: Vec<(String, CategoryStats)>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CategoryStats {
    pub turns: u64,
    pub cost_usd: f64,
    pub duration_seconds: f64,
    pub retries: u64,
    pub edit_turns: u64,
    pub one_shot_turns: u64,
}

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub project: String,
    pub project_path: String,
    pub sessions: Vec<SessionSummary>,
    pub total_cost_usd: f64,
    pub total_api_calls: u64,
    pub total_files_changed: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub total_duration_seconds: f64,
    pub bash_duration_seconds: f64,
}

#[derive(Debug)]
pub struct Report {
    pub label: String,
    pub total_cost_usd: f64,
    pub total_api_calls: u64,
    pub total_sessions: u64,
    pub total_tokens: TokenUsage,
    pub total_duration_seconds: f64,
    pub bash_duration_seconds: f64,
    pub total_files_changed: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub cache_hit_pct: f64,
    pub projects: Vec<ProjectSummary>,
    pub model_breakdown: Vec<(String, ModelStats)>,
    pub category_breakdown: Vec<(String, CategoryStats)>,
    pub tool_breakdown: Vec<(String, u64)>,
    pub mcp_breakdown: Vec<(String, u64)>,
    pub bash_breakdown: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ModelStats {
    pub calls: u64,
    pub cost_usd: f64,
    pub tokens: TokenUsage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_spec_from_period() {
        let spec = DateSpec::from(Period::Today);
        let (range, label) = spec.date_range();
        assert!(label.starts_with("Today"));
        assert!(range.end > range.start);
    }

    #[test]
    fn test_date_spec_custom_range() {
        let start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 3, 31).unwrap();
        let spec = DateSpec::Custom {
            start,
            end,
            label: Some("Q1 2025".into()),
        };
        let (range, label) = spec.date_range();
        assert_eq!(range.start, start);
        assert_eq!(range.end, end);
        assert_eq!(label, "Q1 2025");
    }

    #[test]
    fn test_date_spec_custom_auto_label() {
        let start = NaiveDate::from_ymd_opt(2025, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 6, 30).unwrap();
        let spec = DateSpec::Custom {
            start,
            end,
            label: None,
        };
        let (_, label) = spec.date_range();
        assert_eq!(label, "2025-06-01 to 2025-06-30");
    }

    #[test]
    fn test_period_cycle() {
        assert_eq!(Period::Today.next(), Period::Week);
        assert_eq!(Period::Week.next(), Period::Days30);
        assert_eq!(Period::Days30.next(), Period::Month);
        assert_eq!(Period::Month.next(), Period::Today);
        assert_eq!(Period::Today.prev(), Period::Month);
    }
}
