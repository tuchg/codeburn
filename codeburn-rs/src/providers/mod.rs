pub mod types;
pub mod claude;
pub mod codex;
pub mod cursor;
pub mod opencode;

use types::{Provider, SessionSource};

pub fn get_all_providers(claude_dir: &std::path::Path) -> Vec<Box<dyn Provider>> {
    let providers: Vec<Box<dyn Provider>> = vec![
        Box::new(claude::ClaudeProvider::new(claude_dir.to_path_buf())),
        Box::new(codex::CodexProvider::new(codex::CodexProvider::default_dir())),
        Box::new(cursor::CursorProvider::new(None)),
        Box::new(opencode::OpenCodeProvider::new(opencode::OpenCodeProvider::default_dir())),
    ];
    providers
}

pub fn get_provider(
    name: &str,
    claude_dir: &std::path::Path,
) -> Option<Box<dyn Provider>> {
    match name {
        "claude" => Some(Box::new(claude::ClaudeProvider::new(claude_dir.to_path_buf()))),
        "codex" => Some(Box::new(codex::CodexProvider::new(codex::CodexProvider::default_dir()))),
        "cursor" => Some(Box::new(cursor::CursorProvider::new(None))),
        "opencode" => Some(Box::new(opencode::OpenCodeProvider::new(opencode::OpenCodeProvider::default_dir()))),
        _ => None,
    }
}

pub fn discover_all_sessions(
    claude_dir: &std::path::Path,
    provider_filter: Option<&str>,
) -> Vec<SessionSource> {
    let all_providers = get_all_providers(claude_dir);
    let filtered: Vec<&Box<dyn Provider>> = match provider_filter {
        Some(name) if name != "all" => all_providers.iter().filter(|p| p.name() == name).collect(),
        _ => all_providers.iter().collect(),
    };

    let mut all = Vec::new();
    for provider in filtered {
        let sessions = provider.discover_sessions();
        all.extend(sessions);
    }
    all
}
