use chrono::NaiveDate;
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct JournalEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub message: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ApiUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, rhs: Self) {
        self.input_tokens += rhs.input_tokens;
        self.output_tokens += rhs.output_tokens;
        self.cache_creation_tokens += rhs.cache_creation_tokens;
        self.cache_read_tokens += rhs.cache_read_tokens;
    }
}

#[derive(Debug, Clone)]
pub struct ParsedApiCall {
    pub model: String,
    pub usage: TokenUsage,
    pub cost_usd: f64,
    pub tools: Vec<String>,
    pub timestamp: String,
    pub file_paths: Vec<String>,
    pub lines_added: u64,
    pub lines_removed: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ParsedTurn {
    pub user_message: String,
    pub calls: Vec<ParsedApiCall>,
    pub timestamp: String,
    pub session_id: String,
    pub category: String,
}

#[allow(dead_code)]
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
    pub category_breakdown: Vec<(String, CategoryStats)>,
}

#[derive(Debug, Clone, Default)]
pub struct CategoryStats {
    pub turns: u64,
    pub cost_usd: f64,
    pub duration_seconds: f64,
}

#[allow(dead_code)]
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
}

#[derive(Debug)]
pub struct Report {
    pub label: String,
    pub total_cost_usd: f64,
    pub total_api_calls: u64,
    pub total_tokens: TokenUsage,
    pub total_duration_seconds: f64,
    pub total_files_changed: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub projects: Vec<ProjectSummary>,
    pub model_breakdown: Vec<(String, ModelStats)>,
    pub category_breakdown: Vec<(String, CategoryStats)>,
    pub tool_breakdown: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelStats {
    pub calls: u64,
    pub cost_usd: f64,
    pub tokens: TokenUsage,
}
