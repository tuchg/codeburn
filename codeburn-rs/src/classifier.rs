use regex::Regex;

use crate::bash_utils::is_bash_tool;

const EDIT_TOOLS: &[&str] = &[
    "Edit",
    "edit",
    "Write",
    "write",
    "FileEditTool",
    "FileWriteTool",
    "NotebookEdit",
    "cursor:edit",
];

const READ_TOOLS: &[&str] = &[
    "Read",
    "read",
    "Grep",
    "grep",
    "Glob",
    "glob",
    "FileReadTool",
    "GrepTool",
    "GlobTool",
];

const TASK_TOOLS: &[&str] = &[
    "TaskCreate",
    "TaskUpdate",
    "TaskGet",
    "TaskList",
    "TaskOutput",
    "TaskStop",
    "TodoWrite",
];

const SEARCH_TOOLS: &[&str] = &["WebSearch", "WebFetch", "ToolSearch"];

pub fn has_edit_tools(tools: &[String]) -> bool {
    tools.iter().any(|t| EDIT_TOOLS.contains(&t.as_str()))
}

fn has_read_tools(tools: &[String]) -> bool {
    tools.iter().any(|t| READ_TOOLS.contains(&t.as_str()))
}

fn has_bash_tool(tools: &[String]) -> bool {
    tools.iter().any(|t| is_bash_tool(t))
}

fn has_task_tools(tools: &[String]) -> bool {
    tools.iter().any(|t| TASK_TOOLS.contains(&t.as_str()))
}

fn has_search_tools(tools: &[String]) -> bool {
    tools.iter().any(|t| SEARCH_TOOLS.contains(&t.as_str()))
}

fn has_mcp_tools(tools: &[String]) -> bool {
    tools.iter().any(|t| t.starts_with("mcp__"))
}

fn has_skill_tool(tools: &[String]) -> bool {
    tools.iter().any(|t| t == "Skill")
}

fn has_agent_spawn(tools: &[String]) -> bool {
    tools.iter().any(|t| t == "Agent")
}

fn has_plan_mode(tools: &[String]) -> bool {
    tools.iter().any(|t| t == "EnterPlanMode")
}

fn classify_by_tool_pattern(all_tools: &[String], user_msg: &str) -> Option<String> {
    if all_tools.is_empty() {
        return None;
    }

    if has_plan_mode(all_tools) {
        return Some("planning".to_string());
    }
    if has_agent_spawn(all_tools) {
        return Some("delegation".to_string());
    }

    let has_edits = has_edit_tools(all_tools);
    let has_reads = has_read_tools(all_tools);
    let has_bash = has_bash_tool(all_tools);
    let has_tasks = has_task_tools(all_tools);
    let has_search = has_search_tools(all_tools);
    let has_mcp = has_mcp_tools(all_tools);
    let has_skill = has_skill_tool(all_tools);

    if has_bash && !has_edits {
        let test_re = Regex::new(r"(?i)\b(test|pytest|vitest|jest|mocha|spec|coverage|npm\s+test|npx\s+vitest|npx\s+jest)\b").unwrap();
        let git_re = Regex::new(r"(?i)\bgit\s+(push|pull|commit|merge|rebase|checkout|branch|stash|log|diff|status|add|reset|cherry-pick|tag)\b").unwrap();
        let build_re = Regex::new(r"(?i)\b(npm\s+run\s+build|npm\s+publish|pip\s+install|docker|deploy|make\s+build|npm\s+run\s+dev|npm\s+start|pm2|systemctl|brew|cargo\s+build)\b").unwrap();
        let install_re = Regex::new(r"(?i)\b(npm\s+install|pip\s+install|brew\s+install|apt\s+install|cargo\s+add)\b").unwrap();

        if test_re.is_match(user_msg) {
            return Some("testing".to_string());
        }
        if git_re.is_match(user_msg) {
            return Some("git".to_string());
        }
        if build_re.is_match(user_msg) {
            return Some("build/deploy".to_string());
        }
        if install_re.is_match(user_msg) {
            return Some("build/deploy".to_string());
        }
    }

    if has_edits {
        return Some("coding".to_string());
    }

    if has_bash && has_reads {
        return Some("exploration".to_string());
    }
    if has_bash {
        return Some("coding".to_string());
    }

    if has_search || has_mcp {
        return Some("exploration".to_string());
    }
    if has_reads {
        return Some("exploration".to_string());
    }
    if has_tasks {
        return Some("planning".to_string());
    }
    if has_skill {
        return Some("general".to_string());
    }

    None
}

fn refine_by_keywords(category: &str, user_msg: &str) -> String {
    let debug_re = Regex::new(r"(?i)\b(fix|bug|error|broken|failing|crash|issue|debug|traceback|exception|stack\s*trace|not\s+working|wrong|unexpected|status\s+code|404|500|401|403)\b").unwrap();
    let feature_re = Regex::new(r"(?i)\b(add|create|implement|new|build|feature|introduce|set\s*up|scaffold|generate|make\s+(?:a|me|the)|write\s+(?:a|me|the))\b").unwrap();
    let refactor_re = Regex::new(r"(?i)\b(refactor|clean\s*up|rename|reorganize|simplify|extract|restructure|move|migrate|split)\b").unwrap();
    let research_re = Regex::new(r"(?i)\b(research|investigate|look\s+into|find\s+out|check|search|analyze|review|understand|explain|how\s+does|what\s+is|show\s+me|list|compare)\b").unwrap();

    match category {
        "coding" => {
            if debug_re.is_match(user_msg) {
                return "debugging".to_string();
            }
            if refactor_re.is_match(user_msg) {
                return "refactoring".to_string();
            }
            if feature_re.is_match(user_msg) {
                return "feature".to_string();
            }
            "coding".to_string()
        }
        "exploration" => {
            if research_re.is_match(user_msg) {
                return "exploration".to_string();
            }
            if debug_re.is_match(user_msg) {
                return "debugging".to_string();
            }
            "exploration".to_string()
        }
        _ => category.to_string(),
    }
}

fn classify_conversation(user_msg: &str) -> String {
    let brainstorm_re = Regex::new(r"(?i)\b(brainstorm|idea|what\s+if|explore|think\s+about|approach|strategy|design|consider|how\s+should|what\s+would|opinion|suggest|recommend)\b").unwrap();
    let research_re = Regex::new(r"(?i)\b(research|investigate|look\s+into|find\s+out|check|search|analyze|review|understand|explain|how\s+does|what\s+is|show\s+me|list|compare)\b").unwrap();
    let debug_re = Regex::new(r"(?i)\b(fix|bug|error|broken|failing|crash|issue|debug|traceback|exception|stack\s*trace|not\s+working|wrong|unexpected|status\s+code|404|500|401|403)\b").unwrap();
    let feature_re = Regex::new(r"(?i)\b(add|create|implement|new|build|feature|introduce|set\s*up|scaffold|generate|make\s+(?:a|me|the)|write\s+(?:a|me|the))\b").unwrap();
    let file_re = Regex::new(r"(?i)\.(py|js|ts|tsx|jsx|json|yaml|yml|toml|sql|sh|go|rs|java|rb|php|css|html|md|csv|xml)\b").unwrap();
    let script_re = Regex::new(r"(?i)\b(run\s+\S+\.\w+|execute|scrip?t|curl|api\s+\S+|endpoint|request\s+url|fetch\s+\S+|query|database|db\s+\S+)\b").unwrap();
    let url_re = Regex::new(r"(?i)https?://\S+").unwrap();

    if brainstorm_re.is_match(user_msg) {
        return "brainstorming".to_string();
    }
    if research_re.is_match(user_msg) {
        return "exploration".to_string();
    }
    if debug_re.is_match(user_msg) {
        return "debugging".to_string();
    }
    if feature_re.is_match(user_msg) {
        return "feature".to_string();
    }
    if file_re.is_match(user_msg) {
        return "coding".to_string();
    }
    if script_re.is_match(user_msg) {
        return "coding".to_string();
    }
    if url_re.is_match(user_msg) {
        return "exploration".to_string();
    }
    "conversation".to_string()
}

/// Counts edit-then-bash-then-edit retry cycles
pub fn count_retries(calls: &[crate::types::ParsedApiCall]) -> u64 {
    let mut saw_edit_before_bash = false;
    let mut saw_bash_after_edit = false;
    let mut retries: u64 = 0;

    for call in calls {
        let has_edit = call.tools.iter().any(|t| EDIT_TOOLS.contains(&t.as_str()));
        let has_bash = call.tools.iter().any(|t| is_bash_tool(t));

        if has_edit {
            if saw_bash_after_edit {
                retries += 1;
            }
            saw_edit_before_bash = true;
            saw_bash_after_edit = false;
        }
        if has_bash && saw_edit_before_bash {
            saw_bash_after_edit = true;
        }
    }

    retries
}

/// Classify a turn based on tools and user message
pub fn classify_turn(all_tools: &[String], user_msg: &str) -> String {
    if all_tools.is_empty() {
        classify_conversation(user_msg)
    } else if let Some(tool_category) = classify_by_tool_pattern(all_tools, user_msg) {
        refine_by_keywords(&tool_category, user_msg)
    } else {
        classify_conversation(user_msg)
    }
}

/// Category labels for display (matching TS CATEGORY_LABELS)
pub fn category_label(cat: &str) -> &str {
    match cat {
        "coding" => "Coding",
        "debugging" => "Debugging",
        "feature" => "Feature Dev",
        "refactoring" => "Refactoring",
        "testing" => "Testing",
        "exploration" => "Exploration",
        "planning" => "Planning",
        "delegation" => "Delegation",
        "git" => "Git Ops",
        "build/deploy" => "Build/Deploy",
        "conversation" => "Conversation",
        "brainstorming" => "Brainstorming",
        "general" => "General",
        _ => cat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_tool_classified_as_coding() {
        let tools = vec!["Edit".to_string()];
        assert_eq!(classify_turn(&tools, "update the config"), "coding");
    }

    #[test]
    fn test_edit_with_debug_keyword() {
        let tools = vec!["Edit".to_string()];
        assert_eq!(classify_turn(&tools, "fix the bug in login"), "debugging");
    }

    #[test]
    fn test_edit_with_refactor_keyword() {
        let tools = vec!["Edit".to_string()];
        assert_eq!(classify_turn(&tools, "refactor the auth module"), "refactoring");
    }

    #[test]
    fn test_edit_with_feature_keyword() {
        let tools = vec!["Edit".to_string()];
        assert_eq!(classify_turn(&tools, "add a new login feature"), "feature");
    }

    #[test]
    fn test_bash_only_with_test() {
        let tools = vec!["Bash".to_string()];
        assert_eq!(classify_turn(&tools, "run npm test"), "testing");
    }

    #[test]
    fn test_bash_only_with_git() {
        let tools = vec!["Bash".to_string()];
        assert_eq!(classify_turn(&tools, "git push origin main"), "git");
    }

    #[test]
    fn test_bash_only_with_build() {
        let tools = vec!["Bash".to_string()];
        assert_eq!(classify_turn(&tools, "deploy to production"), "build/deploy");
    }

    #[test]
    fn test_read_only_exploration() {
        let tools = vec!["Read".to_string(), "Grep".to_string()];
        assert_eq!(classify_turn(&tools, "show me the code"), "exploration");
    }

    #[test]
    fn test_no_tools_conversation() {
        let tools: Vec<String> = vec![];
        assert_eq!(classify_turn(&tools, "hello world"), "conversation");
    }

    #[test]
    fn test_no_tools_brainstorming() {
        let tools: Vec<String> = vec![];
        assert_eq!(classify_turn(&tools, "brainstorm ideas for the API"), "brainstorming");
    }

    #[test]
    fn test_delegation() {
        let tools = vec!["Agent".to_string()];
        assert_eq!(classify_turn(&tools, "do something"), "delegation");
    }

    #[test]
    fn test_planning() {
        let tools = vec!["EnterPlanMode".to_string()];
        assert_eq!(classify_turn(&tools, "plan the refactor"), "planning");
    }

    #[test]
    fn test_retry_counting() {
        use crate::types::{ParsedApiCall, TokenUsage};

        let calls = vec![
            ParsedApiCall {
                provider: "claude".into(),
                model: "test".into(),
                usage: TokenUsage::default(),
                cost_usd: 0.0,
                tools: vec!["Edit".into()],
                mcp_tools: vec![],
                bash_commands: vec![],
                timestamp: "".into(),
                file_paths: vec![],
                lines_added: 0,
                lines_removed: 0,
                deduplication_key: "".into(),
            },
            ParsedApiCall {
                provider: "claude".into(),
                model: "test".into(),
                usage: TokenUsage::default(),
                cost_usd: 0.0,
                tools: vec!["Bash".into()],
                mcp_tools: vec![],
                bash_commands: vec![],
                timestamp: "".into(),
                file_paths: vec![],
                lines_added: 0,
                lines_removed: 0,
                deduplication_key: "".into(),
            },
            ParsedApiCall {
                provider: "claude".into(),
                model: "test".into(),
                usage: TokenUsage::default(),
                cost_usd: 0.0,
                tools: vec!["Edit".into()],
                mcp_tools: vec![],
                bash_commands: vec![],
                timestamp: "".into(),
                file_paths: vec![],
                lines_added: 0,
                lines_removed: 0,
                deduplication_key: "".into(),
            },
        ];

        assert_eq!(count_retries(&calls), 1);
    }
}
