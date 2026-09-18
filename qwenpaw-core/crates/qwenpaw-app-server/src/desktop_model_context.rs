//! Preserve legacy context metadata for an explicit, unlisted model.
//! Ported from `providers/context_windows.py`; not a live provider capability probe.

const WINDOWS: &[(&str, u64)] = &[
    ("qwen-long", 10_000_000),
    ("qwen-flash", 1_000_000),
    ("qwen-turbo-latest", 1_000_000),
    ("qwen-turbo", 131_072),
    ("qwen3.8", 1_000_000),
    ("qwen3.7-max", 1_000_000),
    ("qwen3.7-plus", 1_000_000),
    ("qwen3.6-plus", 1_000_000),
    ("qwen-plus-latest", 1_000_000),
    ("qwen-plus", 131_072),
    ("qwen3-coder-plus", 1_000_000),
    ("qwen3-coder", 262_144),
    ("qwen3-max", 262_144),
    ("qwen-max", 131_072),
    ("qwq", 131_072),
    ("claude-instant", 100_000),
    ("claude-2", 100_000),
    ("claude", 200_000),
    ("gpt-4.1", 1_047_576),
    ("gpt-5", 272_000),
    ("o4-mini", 200_000),
    ("o3", 200_000),
    ("gemini-1.5-pro", 2_097_152),
    ("gemini", 1_048_576),
    ("minimax-m3", 1_000_000),
    ("minimax-m2.7", 204_800),
    ("kimi-k3", 1_000_000),
    ("kimi-k2", 262_144),
    ("glm-5.2", 1_000_000),
    ("glm-4.6", 200_000),
    ("grok-4-fast", 2_000_000),
    ("grok-4", 256_000),
];

pub(super) fn uncatalogued(model: &str, use_catalog: bool) -> u64 {
    let normalized = model.to_lowercase();
    if !use_catalog {
        return 128 * 1024;
    }
    WINDOWS
        .iter()
        .filter(|(pattern, _)| {
            normalized.match_indices(pattern).any(|(index, _)| {
                normalized[..index]
                    .chars()
                    .next_back()
                    .is_none_or(|character| !character.is_alphanumeric())
            })
        })
        .max_by_key(|(pattern, _)| pattern.len())
        .map_or(128 * 1024, |(_, tokens)| *tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncatalogued_context_preserves_legacy_patterns_boundaries_and_local_opt_out() {
        for (pattern, tokens) in WINDOWS {
            assert_eq!(uncatalogued(pattern, true), *tokens);
            assert_eq!(
                uncatalogued(&format!("vendor/{}-snapshot", pattern.to_uppercase()), true),
                *tokens
            );
            assert_eq!(uncatalogued(pattern, false), 131_072);
        }
        for (model, expected) in [
            ("unknown", 131_072),
            ("gpt-4o3x", 131_072),
            ("éclaude", 131_072),
            ("é/claude-2", 100_000),
            ("gpt-5/claude-2", 100_000),
            ("openai/o3-mini", 200_000),
        ] {
            assert_eq!(uncatalogued(model, true), expected);
        }
    }
}
