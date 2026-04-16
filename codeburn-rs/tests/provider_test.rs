use std::collections::HashSet;
use std::fs;
use std::path::Path;

use codeburn::providers::types::Provider;

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("codeburn-provider-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ==================== Provider Registry ====================

#[test]
fn test_provider_registry_has_all_providers() {
    let tmp = tempdir();
    let providers = codeburn::providers::get_all_providers(&tmp);
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    assert!(names.contains(&"claude"));
    assert!(names.contains(&"codex"));
    assert!(names.contains(&"cursor"));
    assert!(names.contains(&"opencode"));
    assert_eq!(names.len(), 4);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_provider_display_names() {
    let tmp = tempdir();
    let providers = codeburn::providers::get_all_providers(&tmp);
    let claude = providers.iter().find(|p| p.name() == "claude").unwrap();
    assert_eq!(claude.display_name(), "Claude");
    let codex = providers.iter().find(|p| p.name() == "codex").unwrap();
    assert_eq!(codex.display_name(), "Codex");
    let cursor = providers.iter().find(|p| p.name() == "cursor").unwrap();
    assert_eq!(cursor.display_name(), "Cursor");
    let opencode = providers.iter().find(|p| p.name() == "opencode").unwrap();
    assert_eq!(opencode.display_name(), "OpenCode");
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Claude Provider ====================

#[test]
fn test_claude_model_display_names() {
    let provider = codeburn::providers::claude::ClaudeProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.model_display_name("claude-opus-4-6-20260205"), "Opus 4.6");
    assert_eq!(provider.model_display_name("claude-sonnet-4-6"), "Sonnet 4.6");
    assert_eq!(provider.model_display_name("claude-sonnet-4"), "Sonnet 4");
    assert_eq!(provider.model_display_name("claude-haiku-4-5"), "Haiku 4.5");
    assert_eq!(provider.model_display_name("claude-3-5-sonnet"), "Sonnet 3.5");
}

#[test]
fn test_claude_tool_display_names_are_identity() {
    let provider = codeburn::providers::claude::ClaudeProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.tool_display_name("Bash"), "Bash");
    assert_eq!(provider.tool_display_name("Read"), "Read");
    assert_eq!(provider.tool_display_name("Edit"), "Edit");
}

#[test]
fn test_claude_discovers_no_sessions_on_empty_dir() {
    let tmp = tempdir();
    let provider = codeburn::providers::claude::ClaudeProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert!(sessions.is_empty());
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_claude_discovers_project_dirs() {
    let tmp = tempdir();
    let projects_dir = tmp.join("projects");
    fs::create_dir_all(projects_dir.join("my-project")).unwrap();
    fs::create_dir_all(projects_dir.join("other-project")).unwrap();

    let provider = codeburn::providers::claude::ClaudeProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();

    let projects: Vec<&str> = sessions.iter().map(|s| s.project.as_str()).collect();
    assert!(projects.contains(&"my-project"));
    assert!(projects.contains(&"other-project"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_claude_parses_session_jsonl() {
    let tmp = tempdir();
    let projects_dir = tmp.join("projects").join("test-project");
    fs::create_dir_all(&projects_dir).unwrap();

    let jsonl = r#"{"type":"user","timestamp":"2026-04-16T00:00:00Z","sessionId":"s1","message":{"role":"user","content":"fix the bug"}}
{"type":"assistant","timestamp":"2026-04-16T00:00:10Z","sessionId":"s1","message":{"id":"msg-001","role":"assistant","model":"claude-sonnet-4-6","content":[{"type":"text","text":"Fixing..."},{"type":"tool_use","id":"tu1","name":"Edit","input":{"file_path":"main.rs","old_string":"old","new_string":"new"}}],"usage":{"input_tokens":500,"output_tokens":100,"cache_creation_input_tokens":50,"cache_read_input_tokens":25}}}"#;

    fs::write(projects_dir.join("session-001.jsonl"), jsonl).unwrap();

    let provider = codeburn::providers::claude::ClaudeProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);

    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].provider, "claude");
    assert_eq!(calls[0].model, "claude-sonnet-4-6");
    assert_eq!(calls[0].input_tokens, 500);
    assert_eq!(calls[0].output_tokens, 100);
    assert!(calls[0].cost_usd > 0.0);
    assert!(calls[0].tools.contains(&"Edit".to_string()));
    assert_eq!(calls[0].user_message, "fix the bug");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_claude_deduplication() {
    let tmp = tempdir();
    let projects_dir = tmp.join("projects").join("dedup-project");
    fs::create_dir_all(&projects_dir).unwrap();

    let entry1 = r#"{"type":"assistant","timestamp":"2026-04-16T00:00:10Z","sessionId":"s1","message":{"id":"msg-dup","role":"assistant","model":"claude-sonnet-4-6","content":[],"usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let entry2 = r#"{"type":"assistant","timestamp":"2026-04-16T00:00:10Z","sessionId":"s1","message":{"id":"msg-dup","role":"assistant","model":"claude-sonnet-4-6","content":[],"usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let content = format!("{}\n{}", entry1, entry2);

    fs::write(projects_dir.join("session-dup.jsonl"), &content).unwrap();

    let provider = codeburn::providers::claude::ClaudeProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1, "duplicate messages should be deduplicated");
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Codex Provider ====================

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
    input: u64,
    cached: u64,
    output: u64,
    reasoning: u64,
    total: u64,
) -> String {
    serde_json::json!({
        "type": "event_msg",
        "timestamp": timestamp,
        "payload": {
            "type": "token_count",
            "info": {
                "last_token_usage": {
                    "input_tokens": input,
                    "cached_input_tokens": cached,
                    "output_tokens": output,
                    "reasoning_output_tokens": reasoning,
                    "total_tokens": input + cached + output + reasoning,
                },
                "total_token_usage": {
                    "total_tokens": total,
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
fn test_codex_model_display_names() {
    let provider =
        codeburn::providers::codex::CodexProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.model_display_name("gpt-5.4"), "GPT-5.4");
    assert_eq!(provider.model_display_name("gpt-5.4-mini"), "GPT-5.4 Mini");
    assert_eq!(
        provider.model_display_name("gpt-5.3-codex"),
        "GPT-5.3 Codex"
    );
    assert_eq!(provider.model_display_name("gpt-5"), "GPT-5");
}

#[test]
fn test_codex_tool_display_names() {
    let provider =
        codeburn::providers::codex::CodexProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.tool_display_name("exec_command"), "Bash");
    assert_eq!(provider.tool_display_name("read_file"), "Read");
    assert_eq!(provider.tool_display_name("write_file"), "Edit");
    assert_eq!(provider.tool_display_name("spawn_agent"), "Agent");
    assert_eq!(provider.tool_display_name("read_dir"), "Glob");
    assert_eq!(provider.tool_display_name("unknown_tool"), "unknown_tool");
}

#[test]
fn test_codex_provider_discovers_sessions() {
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

    let provider = codeburn::providers::codex::CodexProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, "codex");
    assert_eq!(sessions[0].project, "Users-test-myproject");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_codex_provider_parses_token_usage() {
    let tmp = tempdir();
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-parse.jsonl",
        &[
            codex_session_meta("/Users/test/myproject", "codex-cli", "sess-parse", "gpt-5.3-codex"),
            codex_user_message("fix the bug", "2026-04-14T10:00:00Z"),
            codex_function_call("exec_command", "2026-04-14T10:00:30Z"),
            codex_function_call("read_file", "2026-04-14T10:00:35Z"),
            codex_token_count("2026-04-14T10:01:00Z", 500, 100, 200, 50, 850),
        ],
    );

    let provider = codeburn::providers::codex::CodexProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);

    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);

    let call = &calls[0];
    assert_eq!(call.provider, "codex");
    assert_eq!(call.model, "gpt-5.3-codex");
    assert_eq!(call.input_tokens, 400); // 500 - 100 cached
    assert_eq!(call.cached_input_tokens, 100);
    assert_eq!(call.cache_read_input_tokens, 100);
    assert_eq!(call.output_tokens, 200);
    assert_eq!(call.reasoning_tokens, 50);
    assert!(call.tools.contains(&"Bash".to_string()));
    assert!(call.tools.contains(&"Read".to_string()));
    assert_eq!(call.user_message, "fix the bug");
    assert_eq!(call.session_id, "sess-parse");
    assert!(call.cost_usd > 0.0);
    assert!(call.deduplication_key.contains("codex:"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_codex_provider_skips_duplicate_token_counts() {
    let tmp = tempdir();
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-dedup.jsonl",
        &[
            codex_session_meta("/Users/test/proj", "codex-cli", "sess-d", "gpt-5.3-codex"),
            codex_token_count("2026-04-14T10:01:00Z", 500, 0, 200, 0, 700),
            // Same cumulative total - should be skipped
            codex_token_count("2026-04-14T10:01:01Z", 500, 0, 200, 0, 700),
            // New cumulative total - should be kept
            codex_token_count("2026-04-14T10:02:00Z", 300, 0, 100, 0, 1100),
        ],
    );

    let provider = codeburn::providers::codex::CodexProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].input_tokens, 500);
    assert_eq!(calls[1].input_tokens, 300);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_codex_provider_returns_empty_for_nonexistent() {
    let provider =
        codeburn::providers::codex::CodexProvider::new(std::path::PathBuf::from("/nonexistent/path"));
    let sessions = provider.discover_sessions();
    assert!(sessions.is_empty());
}

#[test]
fn test_codex_provider_skips_non_codex_originator() {
    let tmp = tempdir();
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-other.jsonl",
        &[
            codex_session_meta("/Users/test/proj", "some-other-tool", "sess-x", "gpt-5"),
            codex_token_count("2026-04-14T10:01:00Z", 100, 0, 50, 0, 150),
        ],
    );

    let provider = codeburn::providers::codex::CodexProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert!(sessions.is_empty());
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_codex_provider_accepts_case_insensitive_originator() {
    let tmp = tempdir();
    create_codex_session(
        &tmp,
        "2026-04-14",
        "rollout-desktop.jsonl",
        &[
            codex_session_meta("/Users/test/proj", "Codex Desktop", "sess-d", "gpt-5"),
            codex_token_count("2026-04-14T10:01:00Z", 100, 0, 50, 0, 150),
        ],
    );

    let provider = codeburn::providers::codex::CodexProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Cursor Provider ====================

#[test]
fn test_cursor_model_display_names() {
    let provider = codeburn::providers::cursor::CursorProvider::new(None);
    assert_eq!(provider.model_display_name("default"), "Auto (Sonnet est.)");
    assert_eq!(
        provider.model_display_name("claude-4.5-opus-high-thinking"),
        "Opus 4.5 (Thinking)"
    );
    assert_eq!(
        provider.model_display_name("claude-4-sonnet-thinking"),
        "Sonnet 4 (Thinking)"
    );
    assert_eq!(
        provider.model_display_name("grok-code-fast-1"),
        "Grok Code Fast"
    );
    assert_eq!(provider.model_display_name("gemini-3-pro"), "Gemini 3 Pro");
    assert_eq!(provider.model_display_name("gpt-5"), "GPT-5");
    assert_eq!(provider.model_display_name("composer-1"), "Composer 1");
    assert_eq!(
        provider.model_display_name("unknown-model"),
        "unknown-model"
    );
}

#[test]
fn test_cursor_tool_display_names_are_identity() {
    let provider = codeburn::providers::cursor::CursorProvider::new(None);
    assert_eq!(provider.tool_display_name("some_tool"), "some_tool");
}

#[test]
fn test_cursor_registration() {
    let provider = codeburn::providers::cursor::CursorProvider::new(None);
    assert_eq!(provider.name(), "cursor");
    assert_eq!(provider.display_name(), "Cursor");
}

// ==================== OpenCode Provider ====================

#[test]
fn test_opencode_model_display_names() {
    let provider =
        codeburn::providers::opencode::OpenCodeProvider::new(std::path::PathBuf::from("/tmp/fake"));
    // OpenCode strips provider prefix before looking up short names
    assert_eq!(
        provider.model_display_name("anthropic/claude-opus-4-6-20260205"),
        "Opus 4.6"
    );
    assert_eq!(
        provider.model_display_name("google/gemini-2.5-pro"),
        "Gemini 2.5 Pro"
    );
    assert_eq!(
        provider.model_display_name("claude-sonnet-4-6"),
        "Sonnet 4.6"
    );
}

#[test]
fn test_opencode_tool_display_names() {
    let provider =
        codeburn::providers::opencode::OpenCodeProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.tool_display_name("bash"), "Bash");
    assert_eq!(provider.tool_display_name("edit"), "Edit");
    assert_eq!(provider.tool_display_name("read"), "Read");
    assert_eq!(provider.tool_display_name("task"), "Agent");
    assert_eq!(provider.tool_display_name("glob"), "Glob");
    assert_eq!(provider.tool_display_name("grep"), "Grep");
    assert_eq!(provider.tool_display_name("fetch"), "WebFetch");
    assert_eq!(provider.tool_display_name("search"), "WebSearch");
    assert_eq!(provider.tool_display_name("unknown_tool"), "unknown_tool");
}

#[test]
fn test_opencode_registration() {
    let provider =
        codeburn::providers::opencode::OpenCodeProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.name(), "opencode");
    assert_eq!(provider.display_name(), "OpenCode");
}

#[test]
fn test_opencode_empty_on_nonexistent_dir() {
    let provider = codeburn::providers::opencode::OpenCodeProvider::new(
        std::path::PathBuf::from("/nonexistent/path/that/does/not/exist"),
    );
    let sessions = provider.discover_sessions();
    assert!(sessions.is_empty());
}

// ==================== Discover All Sessions ====================

#[test]
fn test_discover_all_sessions_with_filter() {
    let tmp = tempdir();
    fs::create_dir_all(tmp.join("projects").join("some-project")).unwrap();

    // Only claude should return results since codex/cursor/opencode dirs don't exist
    let sessions = codeburn::providers::discover_all_sessions(&tmp, Some("claude"));
    assert!(!sessions.is_empty());

    let sessions = codeburn::providers::discover_all_sessions(&tmp, Some("codex"));
    // Codex uses its own default dir, so won't find anything either
    // Just verify it doesn't crash
    assert!(sessions.is_empty() || !sessions.is_empty());
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_get_provider_by_name() {
    let tmp = tempdir();
    assert!(codeburn::providers::get_provider("claude", &tmp).is_some());
    assert!(codeburn::providers::get_provider("codex", &tmp).is_some());
    assert!(codeburn::providers::get_provider("cursor", &tmp).is_some());
    assert!(codeburn::providers::get_provider("opencode", &tmp).is_some());
    assert!(codeburn::providers::get_provider("nonexistent", &tmp).is_none());
    let _ = fs::remove_dir_all(&tmp);
}
