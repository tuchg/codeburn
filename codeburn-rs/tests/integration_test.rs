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

    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
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

    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
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

    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
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

    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
    let session = &projects[0].sessions[0];

    // From 00:00:15 to 00:05:10 = 295 seconds (4m 55s)
    assert!(session.duration_seconds >= 290.0 && session.duration_seconds <= 310.0,
        "Duration was {} seconds", session.duration_seconds);
}

#[test]
fn test_category_classification() {
    let tmp = tempdir();
    create_test_session(&tmp);

    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
    let report = codeburn::stats::build_report(&projects, "Test");

    let categories: Vec<&str> = report.category_breakdown.iter().map(|(c, _)| c.as_str()).collect();
    assert!(categories.contains(&"Feature Dev"), "Expected Feature Dev category, got {:?}", categories);
    assert!(categories.contains(&"Testing"), "Expected Testing category, got {:?}", categories);
}

#[test]
fn test_empty_directory() {
    let tmp = tempdir();
    fs::create_dir_all(tmp.join("projects")).unwrap();

    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
    assert!(projects.is_empty());
}

#[test]
fn test_date_range_filtering() {
    let tmp = tempdir();
    create_test_session(&tmp);

    // Use a date range that doesn't include the test data
    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
    assert!(projects.is_empty(), "Expected no projects for out-of-range date");
}

#[test]
fn test_format_duration() {
    assert_eq!(codeburn::timing::format_duration(0.0), "0s");
    assert_eq!(codeburn::timing::format_duration(45.0), "45s");
    assert_eq!(codeburn::timing::format_duration(125.0), "2m 5s");
    assert_eq!(codeburn::timing::format_duration(3661.0), "1h 1m 1s");
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

    let date_range = codeburn::types::DateRange {
        start: chrono::NaiveDate::from_ymd_opt(2026, 4, 16).unwrap(),
        end: chrono::NaiveDate::from_ymd_opt(2026, 4, 17).unwrap(),
    };

    let projects = codeburn::parser::discover_and_parse(&tmp, &date_range);
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
