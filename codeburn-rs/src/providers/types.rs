/// Types for the provider abstraction layer, matching the TS `providers/types.ts`.

#[derive(Debug, Clone)]
pub struct SessionSource {
    pub path: String,
    pub project: String,
    #[allow(dead_code)]
    pub provider: String,
}

#[derive(Debug, Clone)]
pub struct ParsedProviderCall {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
    pub tools: Vec<String>,
    pub bash_commands: Vec<String>,
    pub timestamp: String,
    #[allow(dead_code)]
    pub speed: String,
    pub deduplication_key: String,
    pub user_message: String,
    pub session_id: String,
}

pub trait Provider {
    fn name(&self) -> &str;
    fn display_name(&self) -> &str;
    fn model_display_name(&self, model: &str) -> String;
    fn tool_display_name(&self, raw_tool: &str) -> String;
    fn discover_sessions(&self) -> Vec<SessionSource>;
    fn parse_session(
        &self,
        source: &SessionSource,
        seen_keys: &mut std::collections::HashSet<String>,
    ) -> Vec<ParsedProviderCall>;
}
