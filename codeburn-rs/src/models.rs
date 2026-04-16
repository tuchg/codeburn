/// Model cost rates per token, matching the TS FALLBACK_PRICING
struct ModelCosts {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
    web_search: f64,
    fast_multiplier: f64,
}

const WEB_SEARCH_COST: f64 = 0.01;

const FALLBACK_PRICING: &[(&str, ModelCosts)] = &[
    ("claude-opus-4-6", ModelCosts { input: 5e-6, output: 25e-6, cache_write: 6.25e-6, cache_read: 0.5e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 6.0 }),
    ("claude-opus-4-5", ModelCosts { input: 5e-6, output: 25e-6, cache_write: 6.25e-6, cache_read: 0.5e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-opus-4-1", ModelCosts { input: 15e-6, output: 75e-6, cache_write: 18.75e-6, cache_read: 1.5e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-opus-4", ModelCosts { input: 15e-6, output: 75e-6, cache_write: 18.75e-6, cache_read: 1.5e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-sonnet-4-6", ModelCosts { input: 3e-6, output: 15e-6, cache_write: 3.75e-6, cache_read: 0.3e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-sonnet-4-5", ModelCosts { input: 3e-6, output: 15e-6, cache_write: 3.75e-6, cache_read: 0.3e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-sonnet-4", ModelCosts { input: 3e-6, output: 15e-6, cache_write: 3.75e-6, cache_read: 0.3e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-3-7-sonnet", ModelCosts { input: 3e-6, output: 15e-6, cache_write: 3.75e-6, cache_read: 0.3e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-3-5-sonnet", ModelCosts { input: 3e-6, output: 15e-6, cache_write: 3.75e-6, cache_read: 0.3e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-haiku-4-5", ModelCosts { input: 1e-6, output: 5e-6, cache_write: 1.25e-6, cache_read: 0.1e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("claude-3-5-haiku", ModelCosts { input: 0.8e-6, output: 4e-6, cache_write: 1.0e-6, cache_read: 0.08e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("gpt-4o-mini", ModelCosts { input: 0.15e-6, output: 0.6e-6, cache_write: 0.15e-6, cache_read: 0.075e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("gpt-4o", ModelCosts { input: 2.5e-6, output: 10e-6, cache_write: 2.5e-6, cache_read: 1.25e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("gemini-2.5-pro", ModelCosts { input: 1.25e-6, output: 10e-6, cache_write: 1.25e-6, cache_read: 0.315e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("gpt-5.4-mini", ModelCosts { input: 0.4e-6, output: 1.6e-6, cache_write: 0.4e-6, cache_read: 0.2e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("gpt-5.4", ModelCosts { input: 2.5e-6, output: 10e-6, cache_write: 2.5e-6, cache_read: 1.25e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("gpt-5.3-codex", ModelCosts { input: 2.5e-6, output: 10e-6, cache_write: 2.5e-6, cache_read: 1.25e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
    ("gpt-5", ModelCosts { input: 2.5e-6, output: 10e-6, cache_write: 2.5e-6, cache_read: 1.25e-6, web_search: WEB_SEARCH_COST, fast_multiplier: 1.0 }),
];

pub fn get_canonical_name(model: &str) -> &str {
    let s = model.split('@').next().unwrap_or(model);
    // Strip trailing date suffix like -20260205
    if s.len() > 9 {
        let suffix = &s[s.len() - 9..];
        if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
            return &s[..s.len() - 9];
        }
    }
    s
}

fn get_model_costs(model: &str) -> Option<&'static ModelCosts> {
    let canonical = get_canonical_name(model);

    for (prefix, costs) in FALLBACK_PRICING {
        if canonical == *prefix || canonical.starts_with(&format!("{}-", prefix)) {
            return Some(costs);
        }
    }
    // Fuzzy: check if canonical starts with any prefix
    for (prefix, costs) in FALLBACK_PRICING {
        if canonical.starts_with(prefix) {
            return Some(costs);
        }
    }
    None
}

pub fn calculate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    web_search_requests: u64,
    speed: &str,
) -> f64 {
    let costs = match get_model_costs(model) {
        Some(c) => c,
        None => return 0.0,
    };

    let multiplier = if speed == "fast" {
        costs.fast_multiplier
    } else {
        1.0
    };

    multiplier
        * (input_tokens as f64 * costs.input
            + output_tokens as f64 * costs.output
            + cache_creation_tokens as f64 * costs.cache_write
            + cache_read_tokens as f64 * costs.cache_read
            + web_search_requests as f64 * costs.web_search)
}

pub fn short_model_name(model: &str) -> String {
    let canonical = get_canonical_name(model);

    let names: &[(&str, &str)] = &[
        ("claude-opus-4-6", "Opus 4.6"),
        ("claude-opus-4-5", "Opus 4.5"),
        ("claude-opus-4-1", "Opus 4.1"),
        ("claude-opus-4", "Opus 4"),
        ("claude-sonnet-4-6", "Sonnet 4.6"),
        ("claude-sonnet-4-5", "Sonnet 4.5"),
        ("claude-sonnet-4", "Sonnet 4"),
        ("claude-3-7-sonnet", "Sonnet 3.7"),
        ("claude-3-5-sonnet", "Sonnet 3.5"),
        ("claude-haiku-4-5", "Haiku 4.5"),
        ("claude-3-5-haiku", "Haiku 3.5"),
        ("gpt-4o-mini", "GPT-4o Mini"),
        ("gpt-4o", "GPT-4o"),
        ("gpt-5.4-mini", "GPT-5.4 Mini"),
        ("gpt-5.4", "GPT-5.4"),
        ("gpt-5.3-codex", "GPT-5.3 Codex"),
        ("gpt-5", "GPT-5"),
        ("gemini-2.5-pro", "Gemini 2.5 Pro"),
    ];

    for (prefix, name) in names {
        if canonical.starts_with(prefix) {
            return name.to_string();
        }
    }

    canonical.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_sonnet_cost() {
        let cost = calculate_cost("claude-sonnet-4-6", 1000, 500, 0, 0, 0, "standard");
        assert!(cost > 0.0);
        // 1000 * 3e-6 + 500 * 15e-6 = 0.003 + 0.0075 = 0.0105
        assert!((cost - 0.0105).abs() < 1e-10);
    }

    #[test]
    fn test_short_model_name_with_date_suffix() {
        assert_eq!(short_model_name("claude-opus-4-6-20260205"), "Opus 4.6");
    }

    #[test]
    fn test_short_model_name_gpt() {
        assert_eq!(short_model_name("gpt-5.3-codex"), "GPT-5.3 Codex");
    }

    #[test]
    fn test_unknown_model_returns_zero_cost() {
        let cost = calculate_cost("unknown-model-xyz", 1000, 500, 0, 0, 0, "standard");
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_fast_speed_multiplier() {
        let standard = calculate_cost("claude-opus-4-6", 1000, 0, 0, 0, 0, "standard");
        let fast = calculate_cost("claude-opus-4-6", 1000, 0, 0, 0, 0, "fast");
        assert!((fast / standard - 6.0).abs() < 1e-10);
    }
}
