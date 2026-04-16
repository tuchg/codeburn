pub mod bash_utils;
pub mod classifier;
pub mod display;
pub mod export;
pub mod models;
pub mod parser;
pub mod providers;
pub mod stats;
pub mod timing;
pub mod tui;
pub mod types;

/// Unified error type for codeburn operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Export error: {0}")]
    Export(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
