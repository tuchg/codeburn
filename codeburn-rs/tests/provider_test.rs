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
    let providers = codeburn::providers::get_all_providers();
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    assert!(names.contains(&"codex"));
    assert!(names.contains(&"cursor"));
    assert!(names.contains(&"opencode"));
    assert!(names.contains(&"gemini"));
    assert!(names.contains(&"copilot"));
    assert!(names.contains(&"pi"));
    assert_eq!(names.len(), 6);
}

#[test]
fn test_provider_display_names() {
    let providers = codeburn::providers::get_all_providers();
    for p in &providers {
        assert!(!p.display_name().is_empty());
    }
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
    assert!(codeburn::providers::get_provider("codex").is_some());
    assert!(codeburn::providers::get_provider("cursor").is_some());
    assert!(codeburn::providers::get_provider("opencode").is_some());
    assert!(codeburn::providers::get_provider("gemini").is_some());
    assert!(codeburn::providers::get_provider("copilot").is_some());
    assert!(codeburn::providers::get_provider("pi").is_some());
    assert!(codeburn::providers::get_provider("nonexistent").is_none());
    assert!(codeburn::providers::get_provider("claude").is_none());
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
    assert_eq!(provider.model_display_name("gpt-5.3-codex"), "GPT-5.3 Codex");
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
    assert_eq!(provider.model_display_name("anthropic/claude-opus-4-6-20260205"), "Opus 4.6");
    assert_eq!(provider.model_display_name("google/gemini-2.5-pro"), "Gemini 2.5 Pro");
    assert_eq!(provider.model_display_name("claude-sonnet-4-6"), "Sonnet 4.6");
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

// ==================== Gemini Provider ====================

#[test]
fn test_gemini_registration() {
    let provider =
        codeburn::providers::gemini::GeminiProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.name(), "gemini");
    assert_eq!(provider.display_name(), "Gemini");
}

#[test]
fn test_gemini_empty_on_missing_projects_file() {
    let tmp = tempdir();
    let provider = codeburn::providers::gemini::GeminiProvider::new(tmp.clone());
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

    let provider = codeburn::providers::gemini::GeminiProvider::new(tmp.clone());
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

    let provider = codeburn::providers::gemini::GeminiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    let mut seen = HashSet::new();
    let calls = provider.parse_session(&sessions[0], &mut seen);
    assert_eq!(calls.len(), 1, "duplicate message IDs should be deduplicated");
    let _ = fs::remove_dir_all(&tmp);
}

// ==================== Copilot Provider ====================

#[test]
fn test_copilot_registration() {
    let provider = codeburn::providers::copilot::CopilotProvider::new(
        std::path::PathBuf::from("/tmp/fake"),
    );
    assert_eq!(provider.name(), "copilot");
    assert_eq!(provider.display_name(), "Copilot");
}

#[test]
fn test_copilot_model_display_names() {
    let provider = codeburn::providers::copilot::CopilotProvider::new(
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
    let provider = codeburn::providers::copilot::CopilotProvider::new(
        std::path::PathBuf::from("/tmp/fake"),
    );
    assert_eq!(provider.tool_display_name("bash"), "Bash");
    assert_eq!(provider.tool_display_name("read_file"), "Read");
    assert_eq!(provider.tool_display_name("edit_file"), "Edit");
    assert_eq!(provider.tool_display_name("unknown_tool"), "unknown_tool");
}

#[test]
fn test_copilot_empty_on_missing_dir() {
    let provider = codeburn::providers::copilot::CopilotProvider::new(
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

    let provider = codeburn::providers::copilot::CopilotProvider::new(tmp.clone());
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
        codeburn::providers::pi::PiProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.name(), "pi");
    assert_eq!(provider.display_name(), "Pi");
}

#[test]
fn test_pi_model_display_names() {
    let provider =
        codeburn::providers::pi::PiProvider::new(std::path::PathBuf::from("/tmp/fake"));
    assert_eq!(provider.model_display_name("gpt-5"), "GPT-5");
    assert_eq!(provider.model_display_name("gpt-5.4"), "GPT-5.4");
    assert_eq!(provider.model_display_name("gpt-4o"), "GPT-4o");
    assert_eq!(provider.model_display_name("unknown"), "unknown");
}

#[test]
fn test_pi_empty_on_missing_dir() {
    let provider =
        codeburn::providers::pi::PiProvider::new(std::path::PathBuf::from("/nonexistent/pi"));
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

    let provider = codeburn::providers::pi::PiProvider::new(tmp.clone());
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

    let provider = codeburn::providers::pi::PiProvider::new(tmp.clone());
    let sessions = provider.discover_sessions();
    assert!(sessions.is_empty(), "should skip files without session header");
    let _ = fs::remove_dir_all(&tmp);
}
