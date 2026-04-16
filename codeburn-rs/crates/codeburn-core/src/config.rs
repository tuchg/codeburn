use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use tracing::{debug, warn};

/// Per-model pricing override (per-token rates in USD).
#[derive(Debug, Clone, Deserialize)]
pub struct PricingOverride {
    pub model: String,
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_write: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub web_search: f64,
    #[serde(default = "default_multiplier")]
    pub fast_multiplier: f64,
}

fn default_multiplier() -> f64 {
    1.0
}

/// Top-level config file structure.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub pricing: Vec<PricingOverride>,
}

impl Config {
    /// Default config file path (platform-dependent via `dirs::config_dir()`):
    /// - Linux: `~/.config/codeburn/config.toml`
    /// - macOS: `~/Library/Application Support/codeburn/config.toml`
    /// - Windows: `{FOLDERID_RoamingAppData}\codeburn\config.toml`
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("codeburn").join("config.toml"))
    }

    /// Load config from the default path, returning an empty config if the file
    /// does not exist. Returns an error only for malformed TOML.
    pub fn load() -> Result<Self, String> {
        match Self::default_path() {
            Some(path) if path.exists() => {
                debug!(path = %path.display(), "loading config");
                Self::load_from(&path)
            }
            _ => {
                debug!("no config file found, using defaults");
                Ok(Self::default())
            }
        }
    }

    /// Load config from a specific path.
    pub fn load_from(path: &std::path::Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| {
                warn!(path = %path.display(), error = %e, "failed to read config");
                format!("failed to read {}: {}", path.display(), e)
            })?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| {
                warn!(path = %path.display(), error = %e, "failed to parse config");
                format!("failed to parse {}: {}", path.display(), e)
            })?;
        debug!(overrides = config.pricing.len(), "config loaded");
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.pricing.is_empty());
    }

    #[test]
    fn test_pricing_override() {
        let toml_str = r#"
[[pricing]]
model = "my-custom-model"
input = 5e-6
output = 25e-6
cache_write = 6.25e-6
cache_read = 0.5e-6
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.pricing.len(), 1);
        assert_eq!(config.pricing[0].model, "my-custom-model");
        assert!((config.pricing[0].input - 5e-6).abs() < 1e-12);
        assert!((config.pricing[0].output - 25e-6).abs() < 1e-12);
        assert!((config.pricing[0].fast_multiplier - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_multiple_pricing_overrides() {
        let toml_str = r#"
[[pricing]]
model = "model-a"
input = 1e-6
output = 2e-6

[[pricing]]
model = "model-b"
input = 3e-6
output = 6e-6
fast_multiplier = 2.0
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.pricing.len(), 2);
        assert!((config.pricing[1].fast_multiplier - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let config = Config::load().unwrap();
        assert!(config.pricing.is_empty());
    }
}
