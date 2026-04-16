pub mod types;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod gemini;
pub mod opencode;
pub mod pi;

pub fn get_all_providers() -> Vec<Box<dyn types::Provider + Send + Sync>> {
    vec![
        Box::new(codex::CodexProvider::new(codex::CodexProvider::default_dir())),
        Box::new(cursor::CursorProvider::new(None)),
        Box::new(opencode::OpenCodeProvider::new(
            opencode::OpenCodeProvider::default_dir(),
        )),
        Box::new(gemini::GeminiProvider::new(
            gemini::GeminiProvider::default_dir(),
        )),
        Box::new(copilot::CopilotProvider::new(
            copilot::CopilotProvider::default_dir(),
        )),
        Box::new(pi::PiProvider::new(pi::PiProvider::default_dir())),
    ]
}

pub fn get_provider(name: &str) -> Option<Box<dyn types::Provider + Send + Sync>> {
    match name {
        "codex" => Some(Box::new(codex::CodexProvider::new(
            codex::CodexProvider::default_dir(),
        ))),
        "cursor" => Some(Box::new(cursor::CursorProvider::new(None))),
        "opencode" => Some(Box::new(opencode::OpenCodeProvider::new(
            opencode::OpenCodeProvider::default_dir(),
        ))),
        "gemini" => Some(Box::new(gemini::GeminiProvider::new(
            gemini::GeminiProvider::default_dir(),
        ))),
        "copilot" => Some(Box::new(copilot::CopilotProvider::new(
            copilot::CopilotProvider::default_dir(),
        ))),
        "pi" => Some(Box::new(pi::PiProvider::new(pi::PiProvider::default_dir()))),
        _ => None,
    }
}
