use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use codeburn_core::providers::types::Provider;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn tempdir() -> std::path::PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "codeburn-provider-test-{}-{}",
        std::process::id(),
        id
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ==================== Provider Registry ====================

#[test]
fn test_provider_registry_has_all_providers() {
    let providers = codeburn_core::providers::get_all_providers();
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    assert!(names.contains(&"claude"));
    assert!(names.contains(&"codex"));
    assert!(names.contains(&"cursor"));
    assert!(names.contains(&"opencode"));
    assert!(names.contains(&"gemini"));
    assert!(names.contains(&"copilot"));
    assert!(names.contains(&"pi"));
    assert_eq!(names.len(), 7);
}

#[test]
fn test_provider_display_names() {
    let providers = codeburn_core::providers::get_all_providers();
    for p in &providers {
        assert!(!p.display_name().is_empty());
    }
    let claude = providers.iter().find(|p| p.name() == "claude").unwrap();
    assert_eq!(claude.display_name(), "Claude");
    let codex = providers.iter().find(|p| p.name() == "codex").unwrap();
    assert_eq!(codex.display_name(), "Codex");
    let cursor = providers.iter().find(|p| p.name() == "cursor").unwrap();
    assert_eq!(cursor.display_name(), "Cursor");
    let opencode = providers.iter().find(|p| p.name() == "opencode").unwrap();
    assert_eq!(opencode.display_name(), "OpenCode");
    let gemini = providers.iter().find(|p| p.name() == "gemini").unwrap();
    assert_eq!(gemini.display_name(), "Gemini");
    let copilot = providers.iter().find(|p| p.name() == "copilot").unwrap();
    assert_eq!(copilot.display_name(), "Copilot");
    let pi = providers.iter().find(|p| p.name() == "pi").unwrap();
    assert_eq!(pi.display_name(), "Pi");
}

#[test]
fn test_get_provider_by_name() {
    assert!(codeburn_core::providers::get_provider("claude").is_some());
    assert!(codeburn_core::providers::get_provider("codex").is_some());
    assert!(codeburn_core::providers::get_provider("cursor").is_some());
    assert!(codeburn_core::providers::get_provider("opencode").is_some());
    assert!(codeburn_core::providers::get_provider("gemini").is_some());
    assert!(codeburn_core::providers::get_provider("copilot").is_some());
    assert!(codeburn_core::providers::get_provider("pi").is_some());
    assert!(codeburn_core::providers::get_provider("nonexistent").is_none());
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
        codeburn_core::providers::codex::CodexProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.model_display_name("gpt-5.4"), "GPT-5.4");
    assert_eq!(provider.model_display_name("gpt-5.4-mini"), "GPT-5.4 Mini");
    assert_eq!(provider.model_display_name("gpt-5.3-codex"), "GPT-5.3 Codex");
    assert_eq!(provider.model_display_name("gpt-5"), "GPT-5");
}

#[test]
fn test_codex_tool_display_names() {
    let provider =
        codeburn_core::providers::codex::CodexProvider::new(std::path::PathBuf::from("/tmp/fake"));
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

    let provider = codeburn_core::providers::codex::CodexProvider::new(tmp.clone());
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

    let provider = codeburn_core::providers::codex::CodexProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);

    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);

    let call = &calls[0];
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
            codex_token_count("2026-04-14T10:01:01Z", 500, 0, 200, 0, 700),
            codex_token_count("2026-04-14T10:02:00Z", 300, 0, 100, 0, 1100),
        ],
    );

    let provider = codeburn_core::providers::codex::CodexProvider::new(tmp.clone());
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
        codeburn_core::providers::codex::CodexProvider::new(std::path::PathBuf::from("/nonexistent/path"));
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

    let provider = codeburn_core::providers::codex::CodexProvider::new(tmp.clone());
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

    let provider = codeburn_core::providers::codex::CodexProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Cursor Provider ====================

#[test]
fn test_cursor_model_display_names() {
    let provider = codeburn_core::providers::cursor::CursorProvider::new(None);
    assert_eq!(provider.model_display_name("default"), "Auto (Sonnet est.)");
    assert_eq!(provider.model_display_name("claude-4.5-opus-high-thinking"), "Opus 4.5 (Thinking)");
    assert_eq!(provider.model_display_name("claude-4-sonnet-thinking"), "Sonnet 4 (Thinking)");
    assert_eq!(provider.model_display_name("grok-code-fast-1"), "Grok Code Fast");
    assert_eq!(provider.model_display_name("gemini-3-pro"), "Gemini 3 Pro");
    assert_eq!(provider.model_display_name("gpt-5"), "GPT-5");
    assert_eq!(provider.model_display_name("composer-1"), "Composer 1");
    assert_eq!(provider.model_display_name("unknown-model"), "unknown-model");
}

#[test]
fn test_cursor_tool_display_names_are_identity() {
    let provider = codeburn_core::providers::cursor::CursorProvider::new(None);
    assert_eq!(provider.tool_display_name("some_tool"), "some_tool");
}

#[test]
fn test_cursor_registration() {
    let provider = codeburn_core::providers::cursor::CursorProvider::new(None);
    assert_eq!(provider.name(), "cursor");
    assert_eq!(provider.display_name(), "Cursor");
}

// ==================== OpenCode Provider ====================

#[test]
fn test_opencode_model_display_names() {
    let provider =
        codeburn_core::providers::opencode::OpenCodeProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.model_display_name("anthropic/claude-opus-4-6-20260205"), "Opus 4.6");
    assert_eq!(provider.model_display_name("google/gemini-2.5-pro"), "Gemini 2.5 Pro");
    assert_eq!(provider.model_display_name("claude-sonnet-4-6"), "Sonnet 4.6");
}

#[test]
fn test_opencode_tool_display_names() {
    let provider =
        codeburn_core::providers::opencode::OpenCodeProvider::new(std::path::PathBuf::from("/tmp/fake"));
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
        codeburn_core::providers::opencode::OpenCodeProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.name(), "opencode");
    assert_eq!(provider.display_name(), "OpenCode");
}

#[test]
fn test_opencode_empty_on_nonexistent_dir() {
    let provider = codeburn_core::providers::opencode::OpenCodeProvider::new(
        std::path::PathBuf::from("/nonexistent/path/that/does/not/exist"),
    );
    let sessions = provider.discover_sessions();
    assert!(sessions.is_empty());
}

// ==================== Gemini Provider ====================

#[test]
fn test_gemini_registration() {
    let provider =
        codeburn_core::providers::gemini::GeminiProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.name(), "gemini");
    assert_eq!(provider.display_name(), "Gemini");
}

#[test]
fn test_gemini_empty_on_missing_projects_file() {
    let tmp = tempdir();
    let provider = codeburn_core::providers::gemini::GeminiProvider::new(tmp.clone());
    assert!(provider.discover_sessions().is_empty());
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_gemini_discovers_and_parses_sessions() {
    let tmp = tempdir();

    let projects_json = serde_json::json!({
        "projects": {
            "/Users/test/myproject": "myproject"
        }
    });
    fs::write(tmp.join("projects.json"), projects_json.to_string()).unwrap();

    let chats_dir = tmp.join("tmp").join("myproject").join("chats");
    fs::create_dir_all(&chats_dir).unwrap();

    let session_json = serde_json::json!({
        "sessionId": "gemini-sess-001",
        "messages": [
            {
                "type": "user",
                "content": "refactor this code"
            },
            {
                "type": "gemini",
                "id": "msg-001",
                "model": "gemini-2.5-pro",
                "timestamp": "2026-04-16T10:00:00Z",
                "tokens": {
                    "input": 1000,
                    "output": 200,
                    "cached": 100,
                    "thoughts": 50,
                    "total": 1350
                },
                "toolCalls": [
                    {"name": "edit_file"}
                ]
            }
        ]
    });
    fs::write(chats_dir.join("session-001.json"), session_json.to_string()).unwrap();

    let provider = codeburn_core::providers::gemini::GeminiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, "gemini");
    assert_eq!(sessions[0].project, "myproject");

    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);

    let call = &calls[0];
    assert_eq!(call.model, "gemini-2.5-pro");
    assert_eq!(call.input_tokens, 900); // 1000 - 100 cached
    assert_eq!(call.output_tokens, 200);
    assert_eq!(call.cached_input_tokens, 100);
    assert_eq!(call.reasoning_tokens, 50);
    assert!(call.cost_usd > 0.0);
    assert_eq!(call.user_message, "refactor this code");
    assert_eq!(call.session_id, "gemini-sess-001");
    assert!(call.tools.contains(&"edit_file".to_string()));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_gemini_deduplicates_messages() {
    let tmp = tempdir();

    let projects_json = serde_json::json!({"projects": {"/Users/test/proj": "proj"}});
    fs::write(tmp.join("projects.json"), projects_json.to_string()).unwrap();
    let chats_dir = tmp.join("tmp").join("proj").join("chats");
    fs::create_dir_all(&chats_dir).unwrap();

    let session_json = serde_json::json!({
        "sessionId": "s1",
        "messages": [
            {"type": "gemini", "id": "dup-msg", "model": "gemini-2.5-pro",
             "timestamp": "2026-04-16T10:00:00Z",
             "tokens": {"input": 100, "output": 50, "cached": 0, "thoughts": 0}},
            {"type": "gemini", "id": "dup-msg", "model": "gemini-2.5-pro",
             "timestamp": "2026-04-16T10:00:01Z",
             "tokens": {"input": 100, "output": 50, "cached": 0, "thoughts": 0}}
        ]
    });
    fs::write(chats_dir.join("sess.json"), session_json.to_string()).unwrap();

    let provider = codeburn_core::providers::gemini::GeminiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1, "duplicate message IDs should be deduplicated");
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Copilot Provider ====================

#[test]
fn test_copilot_registration() {
    let provider = codeburn_core::providers::copilot::CopilotProvider::new(
        std::path::PathBuf::from("/tmp/fake"),
    );
    assert_eq!(provider.name(), "copilot");
    assert_eq!(provider.display_name(), "Copilot");
}

#[test]
fn test_copilot_model_display_names() {
    let provider = codeburn_core::providers::copilot::CopilotProvider::new(
        std::path::PathBuf::from("/tmp/fake"),
    );
    assert_eq!(provider.model_display_name("gpt-4.1"), "GPT-4.1");
    assert_eq!(provider.model_display_name("gpt-4.1-mini"), "GPT-4.1 Mini");
    assert_eq!(provider.model_display_name("gpt-4.1-nano"), "GPT-4.1 Nano");
    assert_eq!(provider.model_display_name("gpt-5"), "GPT-5");
    assert_eq!(provider.model_display_name("gpt-5-mini"), "GPT-5 Mini");
    assert_eq!(provider.model_display_name("o3"), "o3");
    assert_eq!(provider.model_display_name("o4-mini"), "o4-mini");
    assert_eq!(provider.model_display_name("gpt-4.1-preview"), "GPT-4.1"); // startsWith("gpt-4.1-")
    assert_eq!(provider.model_display_name("unknown"), "unknown");
}

#[test]
fn test_copilot_tool_display_names() {
    let provider = codeburn_core::providers::copilot::CopilotProvider::new(
        std::path::PathBuf::from("/tmp/fake"),
    );
    assert_eq!(provider.tool_display_name("bash"), "Bash");
    assert_eq!(provider.tool_display_name("read_file"), "Read");
    assert_eq!(provider.tool_display_name("edit_file"), "Edit");
    assert_eq!(provider.tool_display_name("unknown_tool"), "unknown_tool");
}

#[test]
fn test_copilot_empty_on_missing_dir() {
    let provider = codeburn_core::providers::copilot::CopilotProvider::new(
        std::path::PathBuf::from("/nonexistent/copilot"),
    );
    assert!(provider.discover_sessions().is_empty());
}

#[test]
fn test_copilot_discovers_and_parses_sessions() {
    let tmp = tempdir();
    let session_dir = tmp.join("sess-abc123");
    fs::create_dir_all(&session_dir).unwrap();

    // Write workspace.yaml with cwd
    fs::write(session_dir.join("workspace.yaml"), "cwd: /Users/test/myproject\n").unwrap();

    let events = vec![
        serde_json::json!({"type": "session.model_change", "timestamp": "2026-04-16T10:00:00Z", "data": {"newModel": "gpt-4.1"}}),
        serde_json::json!({"type": "user.message", "timestamp": "2026-04-16T10:00:01Z", "data": {"content": "add tests"}}),
        serde_json::json!({"type": "assistant.message", "timestamp": "2026-04-16T10:00:05Z", "data": {"messageId": "msg-001", "outputTokens": 300, "toolRequests": [{"name": "edit_file", "toolCallId": "tc-1"}]}}),
    ];
    let events_jsonl = events.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n");
    fs::write(session_dir.join("events.jsonl"), events_jsonl).unwrap();

    let provider = codeburn_core::providers::copilot::CopilotProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, "copilot");
    assert_eq!(sessions[0].project, "Users-test-myproject");

    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);

    let call = &calls[0];
    assert_eq!(call.model, "gpt-4.1");
    assert_eq!(call.output_tokens, 300);
    assert_eq!(call.input_tokens, 0);
    assert!(call.cost_usd > 0.0);
    assert_eq!(call.user_message, "add tests");
    assert!(call.tools.contains(&"Edit".to_string()));
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Pi Provider ====================

#[test]
fn test_pi_registration() {
    let provider =
        codeburn_core::providers::pi::PiProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.name(), "pi");
    assert_eq!(provider.display_name(), "Pi");
}

#[test]
fn test_pi_model_display_names() {
    let provider =
        codeburn_core::providers::pi::PiProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.model_display_name("gpt-5"), "GPT-5");
    assert_eq!(provider.model_display_name("gpt-5.4"), "GPT-5.4");
    assert_eq!(provider.model_display_name("gpt-4o"), "GPT-4o");
    assert_eq!(provider.model_display_name("unknown"), "unknown");
}

#[test]
fn test_pi_empty_on_missing_dir() {
    let provider =
        codeburn_core::providers::pi::PiProvider::new(std::path::PathBuf::from("/nonexistent/pi"));
    assert!(provider.discover_sessions().is_empty());
}

#[test]
fn test_pi_discovers_and_parses_sessions() {
    let tmp = tempdir();
    let session_dir = tmp.join("proj-001");
    fs::create_dir_all(&session_dir).unwrap();

    let lines = vec![
        serde_json::json!({"type": "session", "id": "pi-sess-001", "cwd": "/Users/test/myproject", "timestamp": "2026-04-16T10:00:00Z"}),
        serde_json::json!({"type": "message", "id": "entry-1", "timestamp": "2026-04-16T10:00:01Z", "message": {
            "role": "user",
            "content": [{"type": "text", "text": "implement oauth"}]
        }}),
        serde_json::json!({"type": "message", "id": "entry-2", "timestamp": "2026-04-16T10:00:10Z", "message": {
            "role": "assistant",
            "model": "gpt-5",
            "responseId": "resp-001",
            "usage": {"input": 800, "output": 250, "cacheRead": 50, "cacheWrite": 0},
            "content": [
                {"type": "toolCall", "name": "edit_file"}
            ]
        }}),
    ];
    let content = lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
    fs::write(session_dir.join("session.jsonl"), content).unwrap();

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, "pi");
    assert_eq!(sessions[0].project, "myproject");

    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);

    let call = &calls[0];
    assert_eq!(call.model, "gpt-5");
    assert_eq!(call.input_tokens, 800);
    assert_eq!(call.output_tokens, 250);
    assert_eq!(call.cache_read_input_tokens, 50);
    assert!(call.cost_usd > 0.0);
    assert_eq!(call.user_message, "implement oauth");
    assert_eq!(call.session_id, "pi-sess-001");
    assert!(call.tools.contains(&"edit_file".to_string()));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_pi_skips_non_session_first_line() {
    let tmp = tempdir();
    let session_dir = tmp.join("bad-session");
    fs::create_dir_all(&session_dir).unwrap();

    // First line is not type "session"
    let content = serde_json::json!({"type": "message", "role": "user"}).to_string();
    fs::write(session_dir.join("session.jsonl"), content).unwrap();

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert!(sessions.is_empty(), "should skip files without session header");
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Pi Provider - Additional upstream test cases ====================

#[test]
fn test_pi_discovers_multiple_project_dirs() {
    let tmp = tempdir();

    for (dir_name, cwd, session_id) in &[
        ("proj-a", "/Users/test/project-a", "sess-a"),
        ("proj-b", "/Users/test/project-b", "sess-b"),
    ] {
        let session_dir = tmp.join(dir_name);
        fs::create_dir_all(&session_dir).unwrap();
        let lines = vec![
            serde_json::json!({"type": "session", "id": session_id, "cwd": cwd, "timestamp": "2026-04-16T10:00:00Z"}).to_string(),
            serde_json::json!({"type": "message", "id": "e1", "timestamp": "2026-04-16T10:00:01Z", "message": {
                "role": "assistant", "model": "gpt-5", "responseId": format!("resp-{}", session_id),
                "usage": {"input": 100, "output": 50, "cacheRead": 0, "cacheWrite": 0}
            }}).to_string(),
        ];
        fs::write(session_dir.join("session.jsonl"), lines.join("\n")).unwrap();
    }

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 2);
    let mut projects: Vec<&str> = sessions.iter().map(|s| s.project.as_str()).collect();
    projects.sort();
    assert_eq!(projects, &["project-a", "project-b"]);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_pi_skips_non_jsonl_files() {
    let tmp = tempdir();
    let session_dir = tmp.join("proj-x");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(session_dir.join("notes.txt"), "not a session").unwrap();

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    assert!(provider.discover_sessions().is_empty());
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_pi_skips_zero_token_messages() {
    let tmp = tempdir();
    let session_dir = tmp.join("proj-zero");
    fs::create_dir_all(&session_dir).unwrap();
    let lines = vec![
        serde_json::json!({"type": "session", "id": "s1", "cwd": "/x/y", "timestamp": "2026-04-16T10:00:00Z"}).to_string(),
        // zero tokens - should be skipped
        serde_json::json!({"type": "message", "id": "e1", "timestamp": "2026-04-16T10:00:05Z", "message": {
            "role": "assistant", "model": "gpt-5", "responseId": "r0",
            "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
        }}).to_string(),
        // real tokens
        serde_json::json!({"type": "message", "id": "e2", "timestamp": "2026-04-16T10:00:10Z", "message": {
            "role": "assistant", "model": "gpt-5", "responseId": "r1",
            "usage": {"input": 100, "output": 50, "cacheRead": 0, "cacheWrite": 0}
        }}).to_string(),
    ];
    fs::write(session_dir.join("session.jsonl"), lines.join("\n")).unwrap();

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1, "zero-token messages must be skipped");
    assert_eq!(calls[0].input_tokens, 100);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_pi_multi_turn_session() {
    let tmp = tempdir();
    let session_dir = tmp.join("proj-multi");
    fs::create_dir_all(&session_dir).unwrap();
    let lines = vec![
        serde_json::json!({"type": "session", "id": "s-multi", "cwd": "/x/y", "timestamp": "2026-04-16T10:00:00Z"}).to_string(),
        serde_json::json!({"type": "message", "id": "u1", "timestamp": "2026-04-16T10:00:01Z", "message": {
            "role": "user", "content": [{"type": "text", "text": "first question"}]
        }}).to_string(),
        serde_json::json!({"type": "message", "id": "a1", "timestamp": "2026-04-16T10:00:05Z", "message": {
            "role": "assistant", "model": "gpt-5", "responseId": "resp-1",
            "usage": {"input": 500, "output": 100, "cacheRead": 0, "cacheWrite": 0}
        }}).to_string(),
        serde_json::json!({"type": "message", "id": "u2", "timestamp": "2026-04-16T10:01:00Z", "message": {
            "role": "user", "content": [{"type": "text", "text": "second question"}]
        }}).to_string(),
        serde_json::json!({"type": "message", "id": "a2", "timestamp": "2026-04-16T10:01:10Z", "message": {
            "role": "assistant", "model": "gpt-5", "responseId": "resp-2",
            "usage": {"input": 600, "output": 120, "cacheRead": 0, "cacheWrite": 0}
        }}).to_string(),
    ];
    fs::write(session_dir.join("session.jsonl"), lines.join("\n")).unwrap();

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].user_message, "first question");
    assert_eq!(calls[0].input_tokens, 500);
    assert_eq!(calls[1].user_message, "second question");
    assert_eq!(calls[1].input_tokens, 600);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_pi_extracts_bash_commands() {
    let tmp = tempdir();
    let session_dir = tmp.join("proj-bash");
    fs::create_dir_all(&session_dir).unwrap();
    let lines = vec![
        serde_json::json!({"type": "session", "id": "s-bash", "cwd": "/x/y", "timestamp": "2026-04-16T10:00:00Z"}).to_string(),
        serde_json::json!({"type": "message", "id": "a1", "timestamp": "2026-04-16T10:00:05Z", "message": {
            "role": "assistant", "model": "gpt-5", "responseId": "rb1",
            "usage": {"input": 100, "output": 50, "cacheRead": 0, "cacheWrite": 0},
            "content": [
                {"type": "toolCall", "name": "bash", "arguments": {"command": "git status && cargo test"}}
            ]
        }}).to_string(),
    ];
    fs::write(session_dir.join("session.jsonl"), lines.join("\n")).unwrap();

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);
    let cmds = &calls[0].bash_commands;
    assert!(cmds.contains(&"git".to_string()), "bash commands: {:?}", cmds);
    assert!(cmds.contains(&"cargo".to_string()), "bash commands: {:?}", cmds);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_pi_deduplication_across_parses() {
    let tmp = tempdir();
    let session_dir = tmp.join("proj-dup");
    fs::create_dir_all(&session_dir).unwrap();
    let lines = vec![
        serde_json::json!({"type": "session", "id": "s-dup", "cwd": "/x/y", "timestamp": "2026-04-16T10:00:00Z"}).to_string(),
        serde_json::json!({"type": "message", "id": "a1", "timestamp": "2026-04-16T10:00:05Z", "message": {
            "role": "assistant", "model": "gpt-5", "responseId": "resp-dup",
            "usage": {"input": 100, "output": 50, "cacheRead": 0, "cacheWrite": 0}
        }}).to_string(),
    ];
    fs::write(session_dir.join("session.jsonl"), lines.join("\n")).unwrap();

    let provider = codeburn_core::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();

    let first = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(first.len(), 1);
    let second = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(second.len(), 0, "second parse with same seen set should yield nothing");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_pi_handles_missing_session_file() {
    let provider = codeburn_core::providers::pi::PiProvider::new(std::path::PathBuf::from("/tmp/fake-pi"));
    let source = codeburn_core::providers::types::SessionSource {
        path: "/nonexistent/session.jsonl".to_string(),
        project: "test".to_string(),
        provider: "pi".to_string(),
    };
    let calls = provider.parse_session(&source, &mut HashSet::new());
    assert!(calls.is_empty());
}

// ==================== Copilot Provider - Additional upstream test cases ====================

fn copilot_session_dir(tmp: &Path, session_id: &str, lines: &[String], cwd: &str) {
    let session_dir = tmp.join(session_id);
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(session_dir.join("workspace.yaml"), format!("id: {}\ncwd: {}\n", session_id, cwd)).unwrap();
    fs::write(session_dir.join("events.jsonl"), lines.join("\n") + "\n").unwrap();
}

fn copilot_model_change(model: &str) -> String {
    serde_json::json!({"type": "session.model_change", "timestamp": "2026-04-15T10:00:01Z", "data": {"newModel": model}}).to_string()
}

fn copilot_user_msg(content: &str) -> String {
    serde_json::json!({"type": "user.message", "timestamp": "2026-04-15T10:00:10Z", "data": {"content": content}}).to_string()
}

fn copilot_assistant_msg(id: &str, tokens: u64, tools: &[&str], ts: &str) -> String {
    let tool_requests: Vec<serde_json::Value> = tools.iter().map(|t| {
        serde_json::json!({"name": t, "toolCallId": format!("call-{}", t), "type": "function"})
    }).collect();
    serde_json::json!({
        "type": "assistant.message",
        "timestamp": ts,
        "data": {"messageId": id, "outputTokens": tokens, "toolRequests": tool_requests}
    }).to_string()
}

#[test]
fn test_copilot_tracks_model_changes() {
    let tmp = tempdir();
    copilot_session_dir(&tmp, "sess-mc", &[
        serde_json::json!({"type": "session.start", "timestamp": "2026-04-15T10:00:00Z", "data": {"sessionId": "sess-mc"}}).to_string(),
        copilot_model_change("gpt-5-mini"),
        copilot_user_msg("first"),
        copilot_assistant_msg("msg-1", 50, &[], "2026-04-15T10:00:10Z"),
        copilot_model_change("gpt-4.1"),
        copilot_user_msg("second"),
        copilot_assistant_msg("msg-2", 80, &[], "2026-04-15T10:01:00Z"),
    ], "/home/user/proj");

    let provider = codeburn_core::providers::copilot::CopilotProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert_eq!(sessions.len(), 1);
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].model, "gpt-5-mini");
    assert_eq!(calls[1].model, "gpt-4.1");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_copilot_skips_zero_output_token_messages() {
    let tmp = tempdir();
    copilot_session_dir(&tmp, "sess-zero", &[
        copilot_model_change("gpt-4.1"),
        copilot_assistant_msg("msg-empty", 0, &[], "2026-04-15T10:00:10Z"),
        copilot_assistant_msg("msg-real", 42, &[], "2026-04-15T10:00:15Z"),
    ], "/home/user/proj");

    let provider = codeburn_core::providers::copilot::CopilotProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1, "zero-outputToken messages must be skipped");
    assert_eq!(calls[0].output_tokens, 42);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_copilot_deduplication_across_parses() {
    let tmp = tempdir();
    copilot_session_dir(&tmp, "sess-dup2", &[
        copilot_model_change("gpt-4.1"),
        copilot_assistant_msg("msg-dup", 100, &[], "2026-04-15T10:00:10Z"),
    ], "/home/user/proj");

    let provider = codeburn_core::providers::copilot::CopilotProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();

    let first = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(first.len(), 1);
    let second = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(second.len(), 0, "deduplication must prevent re-reading same message");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_copilot_handles_missing_events_file() {
    let provider = codeburn_core::providers::copilot::CopilotProvider::new(std::path::PathBuf::from("/tmp/fake-copilot"));
    let source = codeburn_core::providers::types::SessionSource {
        path: "/nonexistent/events.jsonl".to_string(),
        project: "test".to_string(),
        provider: "copilot".to_string(),
    };
    let calls = provider.parse_session(&source, &mut HashSet::new());
    assert!(calls.is_empty());
}

#[test]
fn test_copilot_extracts_tool_display_names() {
    let tmp = tempdir();
    copilot_session_dir(&tmp, "sess-tools", &[
        copilot_model_change("gpt-4.1"),
        copilot_user_msg("run tests"),
        copilot_assistant_msg("msg-t", 60, &["bash", "read_file", "write_file"], "2026-04-15T10:00:10Z"),
    ], "/home/user/proj");

    let provider = codeburn_core::providers::copilot::CopilotProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tools.contains(&"Bash".to_string()), "tools: {:?}", calls[0].tools);
    assert!(calls[0].tools.contains(&"Read".to_string()), "tools: {:?}", calls[0].tools);
    assert!(calls[0].tools.contains(&"Edit".to_string()), "tools: {:?}", calls[0].tools);
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Gemini Provider - Additional upstream test cases ====================

#[test]
fn test_gemini_multi_message_session() {
    let tmp = tempdir();

    let projects_json = serde_json::json!({"projects": {"/Users/test/proj": "proj"}});
    fs::write(tmp.join("projects.json"), projects_json.to_string()).unwrap();
    let chats_dir = tmp.join("tmp").join("proj").join("chats");
    fs::create_dir_all(&chats_dir).unwrap();

    let session_json = serde_json::json!({
        "sessionId": "s-multi",
        "messages": [
            {"type": "user", "content": [{"text": "first message"}]},
            {"type": "gemini", "id": "msg-1", "model": "gemini-2.5-pro",
             "timestamp": "2026-04-16T12:00:00Z",
             "tokens": {"input": 10, "output": 5, "cached": 0, "thoughts": 0}},
            {"type": "user", "content": [{"text": "second message"}]},
            {"type": "gemini", "id": "msg-2", "model": "gemini-2.5-pro",
             "timestamp": "2026-04-16T12:01:00Z",
             "tokens": {"input": 20, "output": 10, "cached": 0, "thoughts": 0}}
        ]
    });
    fs::write(chats_dir.join("sess.json"), session_json.to_string()).unwrap();

    let provider = codeburn_core::providers::gemini::GeminiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].user_message, "first message");
    assert_eq!(calls[0].input_tokens, 10);
    assert_eq!(calls[1].user_message, "second message");
    assert_eq!(calls[1].input_tokens, 20);
    // second parse with same seen set yields nothing
    let calls2 = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls2.len(), 0, "deduplication across parses must work");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_gemini_user_message_as_string() {
    // Gemini sometimes stores user content as a plain string, not an array
    let tmp = tempdir();

    let projects_json = serde_json::json!({"projects": {"/Users/test/proj2": "proj2"}});
    fs::write(tmp.join("projects.json"), projects_json.to_string()).unwrap();
    let chats_dir = tmp.join("tmp").join("proj2").join("chats");
    fs::create_dir_all(&chats_dir).unwrap();

    let session_json = serde_json::json!({
        "sessionId": "s-str",
        "messages": [
            {"type": "user", "content": "plain string user message"},
            {"type": "gemini", "id": "msg-s1", "model": "gemini-2.5-pro",
             "timestamp": "2026-04-16T12:00:00Z",
             "tokens": {"input": 100, "output": 50, "cached": 0, "thoughts": 0}}
        ]
    });
    fs::write(chats_dir.join("sess.json"), session_json.to_string()).unwrap();

    let provider = codeburn_core::providers::gemini::GeminiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].user_message, "plain string user message");
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Types: Period and ProviderKind enums ====================

#[test]
fn test_period_enum_values() {
    use codeburn_core::types::Period;
    use clap::ValueEnum;
    // All periods must be expressible as CLI strings
    assert!(Period::from_str("week", true).is_ok());
    assert!(Period::from_str("today", true).is_ok());
    assert!(Period::from_str("30days", true).is_ok());
    assert!(Period::from_str("month", true).is_ok());
    assert!(Period::from_str("all", true).is_ok());
    assert!(Period::from_str("invalid", true).is_err());
}

#[test]
fn test_provider_kind_as_str() {
    use codeburn_core::types::ProviderKind;
    assert_eq!(ProviderKind::Claude.as_str(), "claude");
    assert_eq!(ProviderKind::Codex.as_str(), "codex");
    assert_eq!(ProviderKind::Cursor.as_str(), "cursor");
    assert_eq!(ProviderKind::Opencode.as_str(), "opencode");
    assert_eq!(ProviderKind::Gemini.as_str(), "gemini");
    assert_eq!(ProviderKind::Copilot.as_str(), "copilot");
    assert_eq!(ProviderKind::Pi.as_str(), "pi");
}

#[test]
fn test_provider_kind_cli_parsing() {
    use codeburn_core::types::ProviderKind;
    use clap::ValueEnum;
    assert!(ProviderKind::from_str("claude", true).is_ok());
    assert!(ProviderKind::from_str("codex", true).is_ok());
    assert!(ProviderKind::from_str("gemini", true).is_ok());
    assert!(ProviderKind::from_str("copilot", true).is_ok());
    assert!(ProviderKind::from_str("pi", true).is_ok());
    assert!(ProviderKind::from_str("unknown-provider", true).is_err());
}
