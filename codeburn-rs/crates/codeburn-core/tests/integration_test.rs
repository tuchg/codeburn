use std::fs;
use std::path::Path;

fn create_test_session(dir: &Path) {
    let project_dir = dir.join("projects").join("test-project");
    fs::create_dir_all(&project_dir).unwrap();

    let jsonl = r#"{"type":"user","timestamp":"2026-04-16T00:00:00Z","sessionId":"test-001","message":{"role":"user","content":"Add a login feature"}}
{"type":"assistant","timestamp":"2026-04-16T00:00:15Z","sessionId":"test-001","message":{"id":"msg-001","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"text","text":"Adding login."},{"type":"tool_use","id":"tu-001","name":"Edit","input":{"file_path":"src/auth.ts","old_string":"// placeholder","new_string":"function login() {\n  return true\n}"}}],"usage":{"input_tokens":1000,"output_tokens":200,"cache_creation_input_tokens":100,"cache_read_input_tokens":50}}}
{"type":"user","timestamp":"2026-04-16T00:02:00Z","sessionId":"test-001","message":{"role":"user","content":"Write tests for it"}}
{"type":"assistant","timestamp":"2026-04-16T00:02:20Z","sessionId":"test-001","message":{"id":"msg-002","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"text","text":"Writing tests."},{"type":"tool_use","id":"tu-002","name":"Write","input":{"file_path":"tests/auth.test.ts","content":"test('login works', () => {\n  expect(true).toBe(true)\n})"}}],"usage":{"input_tokens":800,"output_tokens":150,"cache_creation_input_tokens":0,"cache_read_input_tokens":200}}}
{"type":"user","timestamp":"2026-04-16T00:05:00Z","sessionId":"test-001","message":{"role":"user","content":"Run npm test"}}
{"type":"assistant","timestamp":"2026-04-16T00:05:10Z","sessionId":"test-001","message":{"id":"msg-003","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"text","text":"Running tests."},{"type":"tool_use","id":"tu-003","name":"Bash","input":{"command":"npm test"}}],"usage":{"input_tokens":500,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":100}}}"#;

    fs::write(project_dir.join("session-test-001.jsonl"), jsonl).unwrap();
}

#[test]
fn test_parse_and_report() {
    let tmp = tempdir();
    create_test_session(&tmp);

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert_eq!(projects.len(), 1);

    let project = &projects[0];
    assert_eq!(project.total_api_calls, 3);
    assert!(project.total_cost_usd > 0.0);
    assert_eq!(project.total_files_changed, 2);
    assert!(project.total_lines_added > 0);
    assert!(project.total_lines_removed > 0);
    assert!(project.total_duration_seconds > 0.0);
}

#[test]
fn test_file_change_tracking() {
    let tmp = tempdir();
    create_test_session(&tmp);

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    let session = &projects[0].sessions[0];

    let mut files: Vec<&str> = session.files_changed.iter().map(|s| s.as_str()).collect();
    files.sort();

    assert_eq!(files.len(), 2);
    assert!(files.contains(&"src/auth.ts"));
    assert!(files.contains(&"tests/auth.test.ts"));
}

#[test]
fn test_code_diff_counting() {
    let tmp = tempdir();
    create_test_session(&tmp);

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    let session = &projects[0].sessions[0];

    // Edit: old_string "// placeholder" (1 line removed), new_string "function login() {\n  return true\n}" (3 lines added)
    // Write: content "test('login works', () => {\n  expect(true).toBe(true)\n})" (3 lines added)
    assert_eq!(session.total_lines_added, 6);
    assert_eq!(session.total_lines_removed, 1);
}

#[test]
fn test_session_duration() {
    let tmp = tempdir();
    create_test_session(&tmp);

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    let session = &projects[0].sessions[0];

    // From 00:00:15 to 00:05:10 = 295 seconds (4m 55s)
    assert!(session.duration_seconds >= 290.0 && session.duration_seconds <= 310.0,
        "Duration was {} seconds", session.duration_seconds);
}

#[test]
fn test_category_classification() {
    let tmp = tempdir();
    create_test_session(&tmp);

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    let report = codeburn_core::stats::build_report(&projects, "Test");

    let categories: Vec<&str> = report.category_breakdown.iter().map(|(c, _)| c.as_str()).collect();
    assert!(categories.contains(&"feature"), "Expected feature category, got {:?}", categories);
    assert!(categories.contains(&"testing"), "Expected testing category, got {:?}", categories);
}

#[test]
fn test_empty_directory() {
    let tmp = tempdir();
    fs::create_dir_all(tmp.join("projects")).unwrap();

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert!(projects.is_empty());
}

#[test]
fn test_date_range_filtering() {
    let tmp = tempdir();
    create_test_session(&tmp);

    // Use a date range that doesn't include the test data
    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert!(projects.is_empty(), "Expected no projects for out-of-range date");
}

#[test]
fn test_format_duration() {
    assert_eq!(codeburn_core::timing::format_duration(0.0), "0s");
    assert_eq!(codeburn_core::timing::format_duration(45.0), "45s");
    assert_eq!(codeburn_core::timing::format_duration(125.0), "2m 5s");
    assert_eq!(codeburn_core::timing::format_duration(3661.0), "1h 1m 1s");
}

#[test]
fn test_deduplication() {
    let tmp = tempdir();
    let project_dir = tmp.join("projects").join("dup-project");
    fs::create_dir_all(&project_dir).unwrap();

    // Create a session with duplicate message IDs
    let jsonl = r#"{"type":"user","timestamp":"2026-04-16T00:00:00Z","sessionId":"dup-001","message":{"role":"user","content":"Hello"}}
{"type":"assistant","timestamp":"2026-04-16T00:00:05Z","sessionId":"dup-001","message":{"id":"msg-dup","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"text","text":"Hi!"}],"usage":{"input_tokens":100,"output_tokens":20}}}
{"type":"assistant","timestamp":"2026-04-16T00:00:06Z","sessionId":"dup-001","message":{"id":"msg-dup","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"text","text":"Hi duplicate!"}],"usage":{"input_tokens":100,"output_tokens":20}}}"#;

    fs::write(project_dir.join("session-dup.jsonl"), jsonl).unwrap();

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert_eq!(projects.len(), 1);
    // Should only count 1 API call due to deduplication
    assert_eq!(projects[0].total_api_calls, 1);
}

fn tempdir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "codeburn-test-{}-{}",
        std::process::id(),
        id
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ========== Codex provider tests ==========

fn codex_session_meta(cwd: &str, originator: &str, session_id: &str, model: &str) -> String {
    serde_json::json!({
        "type": "session_meta",
        "timestamp": "2026-04-14T10:00:00Z",
        "payload": {
            "cwd": cwd,
            "originator": originator,
            "session_id": session_id,
            "model": model,
        }
    })
    .to_string()
}

fn codex_token_count(
    timestamp: &str,
    last_input: u64,
    last_cached: u64,
    last_output: u64,
    last_reasoning: u64,
    cumulative_total: u64,
) -> String {
    serde_json::json!({
        "type": "event_msg",
        "timestamp": timestamp,
        "payload": {
            "type": "token_count",
            "info": {
                "last_token_usage": {
                    "input_tokens": last_input,
                    "cached_input_tokens": last_cached,
                    "output_tokens": last_output,
                    "reasoning_output_tokens": last_reasoning,
                    "total_tokens": last_input + last_cached + last_output + last_reasoning,
                },
                "total_token_usage": {
                    "input_tokens": last_input,
                    "cached_input_tokens": last_cached,
                    "output_tokens": last_output,
                    "reasoning_output_tokens": last_reasoning,
                    "total_tokens": cumulative_total,
                }
            }
        }
    })
    .to_string()
}

fn codex_function_call(name: &str, timestamp: &str) -> String {
    serde_json::json!({
        "type": "response_item",
        "timestamp": timestamp,
        "payload": {
            "type": "function_call",
            "name": name,
        }
    })
    .to_string()
}

fn codex_user_message(text: &str, timestamp: &str) -> String {
    serde_json::json!({
        "type": "response_item",
        "timestamp": timestamp,
        "payload": {
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": text}],
        }
    })
    .to_string()
}

fn create_codex_session(dir: &Path, date: &str, filename: &str, lines: &[String]) {
    let parts: Vec<&str> = date.split('-').collect();
    let session_dir = dir
        .join("sessions")
        .join(parts[0])
        .join(parts[1])
        .join(parts[2]);
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(session_dir.join(filename), lines.join("\n") + "\n").unwrap();
}

#[test]
fn test_codex_session_discovery() {
    let tmp = tempdir();
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-abc123.jsonl",
        &[
            codex_session_meta("/Users/test/myproject", "codex-cli", "sess-001", "gpt-5.3-codex"),
            codex_token_count("2026-04-14T10:01:00Z", 100, 0, 50, 0, 150),
        ],
    );

    unsafe { std::env::set_var("CODEX_HOME", tmp.to_str().unwrap()); }
    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 14).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
    };

    // Use a non-existent claude dir since we're testing codex only
    let claude_dir = tmp.join("nonexistent-claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", claude_dir.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);

    assert!(
        !projects.is_empty(),
        "Should discover codex sessions, got empty"
    );
    assert_eq!(projects[0].project, "Users-test-myproject");
    unsafe { std::env::remove_var("CODEX_HOME"); }
}

#[test]
fn test_codex_tool_name_mapping() {
    let tmp = tempdir();
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-tools.jsonl",
        &[
            codex_session_meta("/Users/test/proj", "codex-cli", "sess-tools", "gpt-5.3-codex"),
            codex_user_message("fix the bug", "2026-04-14T10:00:00Z"),
            codex_function_call("exec_command", "2026-04-14T10:00:30Z"),
            codex_function_call("read_file", "2026-04-14T10:00:35Z"),
            codex_token_count("2026-04-14T10:01:00Z", 500, 100, 200, 50, 850),
        ],
    );

    unsafe { std::env::set_var("CODEX_HOME", tmp.to_str().unwrap()); }
    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 14).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
    };

    let claude_dir = tmp.join("nonexistent-claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", claude_dir.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);

    assert!(!projects.is_empty());
    let session = &projects[0].sessions[0];
    assert_eq!(session.api_calls, 1);
    assert!(session.total_cost_usd > 0.0);

    // Check tool name mapping
    let all_tools: Vec<String> = session
        .turns
        .iter()
        .flat_map(|t| t.calls.iter())
        .flat_map(|c| c.tools.clone())
        .collect();
    assert!(all_tools.contains(&"Bash".to_string()), "exec_command should map to Bash");
    assert!(all_tools.contains(&"Read".to_string()), "read_file should map to Read");

    unsafe { std::env::remove_var("CODEX_HOME"); }
}

#[test]
fn test_codex_dedup_same_cumulative_total() {
    let tmp = tempdir();
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-dedup.jsonl",
        &[
            codex_session_meta("/Users/test/proj", "codex-cli", "sess-dedup", "gpt-5.3-codex"),
            codex_token_count("2026-04-14T10:01:00Z", 500, 0, 200, 0, 700),
            codex_token_count("2026-04-14T10:01:01Z", 500, 0, 200, 0, 700), // duplicate
            codex_token_count("2026-04-14T10:02:00Z", 300, 0, 100, 0, 1100), // new
        ],
    );

    unsafe { std::env::set_var("CODEX_HOME", tmp.to_str().unwrap()); }
    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 14).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
    };

    let claude_dir = tmp.join("nonexistent-claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", claude_dir.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);

    assert!(!projects.is_empty());
    // Should only produce 2 calls, not 3 (dedup by cumulative total)
    assert_eq!(projects[0].total_api_calls, 2);

    unsafe { std::env::remove_var("CODEX_HOME"); }
}

#[test]
fn test_codex_skips_non_codex_originator() {
    let tmp = tempdir();
    let meta = serde_json::json!({
        "type": "session_meta",
        "timestamp": "2026-04-14T10:00:00Z",
        "payload": {
            "cwd": "/test",
            "originator": "not-codex",
            "session_id": "sess-skip",
            "model": "gpt-5",
        }
    });
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-skip.jsonl",
        &[meta.to_string()],
    );

    unsafe { std::env::set_var("CODEX_HOME", tmp.to_str().unwrap()); }
    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 14).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
    };

    let claude_dir = tmp.join("nonexistent-claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", claude_dir.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert!(projects.is_empty(), "Should skip non-codex session");

    unsafe { std::env::remove_var("CODEX_HOME"); }
}

// ========== Bash breakdown tests ==========

#[test]
fn test_bash_command_breakdown_in_session() {
    let tmp = tempdir();
    let project_dir = tmp.join("projects").join("bash-project");
    fs::create_dir_all(&project_dir).unwrap();

    let jsonl = r#"{"type":"user","timestamp":"2026-04-16T00:00:00Z","sessionId":"bash-001","message":{"role":"user","content":"Run tests"}}
{"type":"assistant","timestamp":"2026-04-16T00:00:05Z","sessionId":"bash-001","message":{"id":"msg-b1","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"text","text":"Running."},{"type":"tool_use","id":"tu-b1","name":"Bash","input":{"command":"cd /project && cargo test && npm run build"}}],"usage":{"input_tokens":100,"output_tokens":20}}}"#;

    fs::write(project_dir.join("session-bash.jsonl"), jsonl).unwrap();

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert_eq!(projects.len(), 1);

    let session = &projects[0].sessions[0];
    let bash_cmds: std::collections::HashMap<&str, u64> = session
        .bash_breakdown
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();

    assert_eq!(bash_cmds.get("cargo"), Some(&1), "Should have cargo");
    assert_eq!(bash_cmds.get("npm"), Some(&1), "Should have npm");
    assert!(bash_cmds.get("cd").is_none(), "Should not have cd");
}

#[test]
fn test_mcp_breakdown_in_session() {
    let tmp = tempdir();
    let project_dir = tmp.join("projects").join("mcp-project");
    fs::create_dir_all(&project_dir).unwrap();

    let jsonl = r#"{"type":"user","timestamp":"2026-04-16T00:00:00Z","sessionId":"mcp-001","message":{"role":"user","content":"Search for issues"}}
{"type":"assistant","timestamp":"2026-04-16T00:00:05Z","sessionId":"mcp-001","message":{"id":"msg-m1","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"text","text":"Searching."},{"type":"tool_use","id":"tu-m1","name":"mcp__github__search_issues","input":{"query":"bug"}},{"type":"tool_use","id":"tu-m2","name":"mcp__github__list_repos","input":{}},{"type":"tool_use","id":"tu-m3","name":"mcp__jira__get_ticket","input":{"id":"PROJ-1"}}],"usage":{"input_tokens":100,"output_tokens":20}}}"#;

    fs::write(project_dir.join("session-mcp.jsonl"), jsonl).unwrap();

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert_eq!(projects.len(), 1);

    let session = &projects[0].sessions[0];
    let mcp_cmds: std::collections::HashMap<&str, u64> = session
        .mcp_breakdown
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();

    assert_eq!(mcp_cmds.get("github"), Some(&2), "Should have 2 github MCP calls");
    assert_eq!(mcp_cmds.get("jira"), Some(&1), "Should have 1 jira MCP call");
}

#[test]
fn test_retry_and_oneshot_tracking() {
    let tmp = tempdir();
    let project_dir = tmp.join("projects").join("retry-project");
    fs::create_dir_all(&project_dir).unwrap();

    // Session with edit -> bash (test) -> edit (retry pattern)
    let jsonl = r#"{"type":"user","timestamp":"2026-04-16T00:00:00Z","sessionId":"retry-001","message":{"role":"user","content":"Add a feature"}}
{"type":"assistant","timestamp":"2026-04-16T00:00:05Z","sessionId":"retry-001","message":{"id":"msg-r1","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"tool_use","id":"tu-r1","name":"Edit","input":{"file_path":"a.ts","old_string":"old","new_string":"new"}}],"usage":{"input_tokens":100,"output_tokens":20}}}
{"type":"assistant","timestamp":"2026-04-16T00:00:10Z","sessionId":"retry-001","message":{"id":"msg-r2","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"tool_use","id":"tu-r2","name":"Bash","input":{"command":"npm test"}}],"usage":{"input_tokens":100,"output_tokens":20}}}
{"type":"assistant","timestamp":"2026-04-16T00:00:15Z","sessionId":"retry-001","message":{"id":"msg-r3","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"tool_use","id":"tu-r3","name":"Edit","input":{"file_path":"a.ts","old_string":"new","new_string":"fixed"}}],"usage":{"input_tokens":100,"output_tokens":20}}}"#;

    fs::write(project_dir.join("session-retry.jsonl"), jsonl).unwrap();

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    assert_eq!(projects.len(), 1);

    let session = &projects[0].sessions[0];
    let turn = &session.turns[0];
    assert_eq!(turn.retries, 1, "Should detect 1 retry cycle");
    assert!(turn.has_edits, "Should detect edits");
}

#[test]
fn test_report_bash_and_mcp_aggregation() {
    let tmp = tempdir();
    let project_dir = tmp.join("projects").join("agg-project");
    fs::create_dir_all(&project_dir).unwrap();

    let jsonl = r#"{"type":"user","timestamp":"2026-04-16T00:00:00Z","sessionId":"agg-001","message":{"role":"user","content":"Do stuff"}}
{"type":"assistant","timestamp":"2026-04-16T00:00:05Z","sessionId":"agg-001","message":{"id":"msg-a1","type":"message","role":"assistant","model":"claude-sonnet-4-20260414","content":[{"type":"tool_use","id":"tu-a1","name":"Bash","input":{"command":"cargo build && cargo test"}},{"type":"tool_use","id":"tu-a2","name":"mcp__github__get_issue","input":{}}],"usage":{"input_tokens":100,"output_tokens":20}}}"#;

    fs::write(project_dir.join("session-agg.jsonl"), jsonl).unwrap();

    let date_range = codeburn_core::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", tmp.to_str().unwrap()); }
    let projects = codeburn_core::parser::discover_and_parse(&date_range, None);
    let report = codeburn_core::stats::build_report(&projects, "Test");

    // Bash breakdown in report
    let bash: std::collections::HashMap<&str, u64> = report
        .bash_breakdown
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();
    assert_eq!(bash.get("cargo"), Some(&2), "Report should aggregate bash commands (cargo x2)");

    // MCP breakdown in report
    let mcp: std::collections::HashMap<&str, u64> = report
        .mcp_breakdown
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();
    assert_eq!(mcp.get("github"), Some(&1), "Report should aggregate MCP calls");

    // Sessions count
    assert_eq!(report.total_sessions, 1);
}
