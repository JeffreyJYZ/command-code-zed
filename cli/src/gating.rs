//! Plan-based model gating, mirroring the opencode plugin's tables
//! (opencode/src/gating.ts + access.ts). Kept compact: category comes from a
//! prefix map; the plugin's exact per-id table drifts, so this is a close
//! approximation and the API enforces the real gate. plan ids absent from
//! RULES allow everything; any purchased/free credits unlock everything.

#[derive(PartialEq)]
pub enum Category {
    OpenSource,
    Premium,
}

/// Longest matching prefix wins. Mirrors MODEL_CATEGORIES for the families
/// that matter; unknown non-claude ids default to OpenSource (the server
/// rejects a truly disallowed model anyway). Matches on the full model path
/// (some premium ids are provider-scoped, e.g. "sakana/fugu-ultra").
fn category(id: &str) -> Category {
    let full = id.to_lowercase();
    const PREMIUM_PREFIXES: &[&str] = &[
        "claude-opus",
        "claude-sonnet",
        "claude-fable",
        "claude-haiku",
        "gpt-5.6-terra",
        "gpt-5.5",
        "gpt-5.4",
        "gpt-5.3-codex",
        "google/gemini-3.5-flash",
        "google/gemini-3.1-flash-lite",
        "sakana/fugu-ultra",
        "meta/muse-spark-1.1",
    ];
    if PREMIUM_PREFIXES.iter().any(|p| full.starts_with(p)) {
        Category::Premium
    } else {
        Category::OpenSource
    }
}

struct Rule {
    open_only: bool,
    /// bare model ids blocked regardless of category (provider qualifier stripped)
    blocked: &'static [&'static str],
}

fn rule(plan_id: &str) -> Option<Rule> {
    match plan_id {
        "individual-go" => Some(Rule {
            open_only: true,
            blocked: &["meta/muse-spark-1.2", "xai/grok-4.6", "google/gemini-3.7-flash", "gpt-5.6-sol"],
        }),
        "individual-goat" => Some(Rule { open_only: true, blocked: &[] }),
        "individual-pro" | "individual-pro-v1" => Some(Rule {
            open_only: false,
            blocked: &["claude-fable-5", "claude-opus-5", "claude-opus-4-8", "claude-opus-4-7", "claude-opus-4-6", "claude-opus-4-5-20251101", "sakana/fugu-ultra"],
        }),
        _ => None, // unknown / max / ultra / provider / team: everything
    }
}

pub fn allowed(model_id: &str, plan_id: &str, unlocked: bool) -> bool {
    check(model_id, plan_id, unlocked).allowed
}

/// Access decision plus a short human reason (for `models --gated --json`).
pub struct Access {
    pub allowed: bool,
    pub reason: &'static str,
}

pub fn check(model_id: &str, plan_id: &str, unlocked: bool) -> Access {
    if unlocked {
        return Access { allowed: true, reason: "credits unlock all models" };
    }
    if plan_id.is_empty() {
        return Access { allowed: true, reason: "unknown plan" };
    }
    let Some(r) = rule(plan_id) else {
        return Access { allowed: true, reason: "plan has no restrictions" };
    };
    let full = model_id.to_lowercase();
    if r.blocked.iter().any(|b| *b == full) {
        return Access { allowed: false, reason: "blocked for this plan" };
    }
    if r.open_only && category(model_id) == Category::Premium {
        return Access { allowed: false, reason: "premium model, plan is open-models-only" };
    }
    Access { allowed: true, reason: "allowed" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goat_blocks_premium_allows_open() {
        assert!(!allowed("claude-opus-5", "individual-goat", false));
        assert!(!allowed("claude-sonnet-5", "individual-goat", false));
        assert!(!allowed("sakana/fugu-ultra", "individual-goat", false));
        assert!(allowed("deepseek/deepseek-v4-pro", "individual-goat", false));
        assert!(allowed("z-ai/glm-5.3-flash", "individual-goat", false));
    }

    #[test]
    fn credits_unlock_everything() {
        assert!(allowed("claude-opus-5", "individual-goat", true));
        assert!(allowed("claude-opus-5", "individual-go", true));
    }

    #[test]
    fn pro_blocks_opus_fable_not_sonnet() {
        assert!(!allowed("claude-opus-5", "individual-pro", false));
        assert!(!allowed("claude-fable-5", "individual-pro", false));
        assert!(!allowed("sakana/fugu-ultra", "individual-pro", false));
        assert!(allowed("claude-sonnet-5", "individual-pro", false));
    }

    #[test]
    fn unknown_plan_allows_all() {
        assert!(allowed("claude-opus-5", "individual-max", false));
        assert!(allowed("claude-opus-5", "", false));
    }

    #[test]
    fn check_reports_reason() {
        assert_eq!(check("claude-opus-5", "individual-goat", false).reason, "premium model, plan is open-models-only");
        assert!(!check("claude-opus-5", "individual-goat", false).allowed);
        let a = check("claude-opus-5", "individual-goat", true);
        assert!(a.allowed);
        assert_eq!(a.reason, "credits unlock all models");
    }
}
