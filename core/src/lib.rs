pub mod dates;
pub mod wire;
pub use dates::parse_iso_utc;
pub use wire::{
    Credits, CreditsResp, SubData, SubscriptionsResp, UsageSummary, Window, WindowLimits,
};

// Plan table, name rules, and monthly caps generated from plans.json
// (single source shared with the TypeScript opencode plugin).
include!(concat!(env!("OUT_DIR"), "/plans.rs"));

/// Plan display name from the API's planId (e.g. "individual-goat" → "GOAT").
/// First rule whose needles are all contained in the lowercased id wins.
pub fn plan_name(plan_id: &str) -> &'static str {
    let id = plan_id.to_lowercase();
    for (needles, name) in NAME_RULES {
        if needles.iter().all(|n| id.contains(n)) {
            return name;
        }
    }
    DEFAULT_NAME
}

/// Monthly credit pool per plan. One shared pool per plan (verified — the
/// docs' per-model allowances are not what the API meters). None = PAYG.
pub fn plan_monthly_cap(plan_id: &str) -> Option<f64> {
    let name = plan_name(plan_id);
    CAPS.iter().find(|(n, _)| *n == name).and_then(|(_, c)| *c)
}

pub fn money(v: f64) -> String {
    format!("${v:.2}")
}

/// Rolling-window lengths — the API serves only `resetAt`, the length is
/// implied by the window name.
pub const FIVE_HOUR_SECS: u64 = 5 * 3600;
pub const WEEKLY_SECS: u64 = 7 * 86400;

pub fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

/// Percent string of used/cap: "34%" or "—" when cap unknown. Rounds
/// half-away-from-zero (`.round()`), matching JS `Math.round` in the opencode
/// port; `format!("{:.0}")` alone is half-to-even and drifts at exact .5.
pub fn pct(used: f64, cap: f64) -> String {
    if cap > 0.0 {
        format!("{:.0}%", ((used / cap) * 100.0).round())
    } else {
        "—".into()
    }
}

/// Compact "Xh Ym" human time until `reset_at` (epoch ms). `now` may be None
/// (zed ext without a clock) → falls back to a raw epoch stamp.
pub fn rel_time(reset_at: Option<f64>, now: Option<u64>) -> String {
    let Some(reset_ms) = reset_at else {
        return "unknown".into();
    };
    let Some(now_s) = now else {
        return format!("epoch {}", reset_ms as u64 / 1000);
    };
    let reset_s = reset_ms as u64 / 1000;
    if reset_s <= now_s {
        // resetAt already passed (clock skew or window rolling over);
        // next fetch will pick up the fresh window
        return "resetting…".into();
    }
    let diff = reset_s - now_s;
    let d = diff / 86400;
    let h = (diff % 86400) / 3600;
    let m = (diff % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        "<1m".into()
    }
}

/// Elapsed % of a rolling window: window length = dur_secs, ends at reset_at.
pub fn elapsed_pct(reset_at: Option<f64>, dur_secs: u64, now: Option<u64>) -> Option<u8> {
    let reset_ms = reset_at?;
    let now_s = now?;
    let reset_s = reset_ms as u64 / 1000;
    let start = reset_s.checked_sub(dur_secs)?;
    if now_s < start {
        return None; // window hasn't started
    }
    let elapsed = now_s - start;
    let pct = (elapsed as f64 / dur_secs as f64 * 100.0).clamp(0.0, 100.0);
    Some(pct.round() as u8)
}

/// Seconds until spend hits cap at the current rate, if that lands before the
/// window resets. None = no warning. Suppressed before 10% of the window has
/// elapsed — the flat-rate projection is unreliable that early.
pub fn pace_eta(
    reset_at: Option<f64>,
    dur_secs: u64,
    used: f64,
    cap: f64,
    now: u64,
) -> Option<f64> {
    let reset_ms = reset_at?;
    let reset_s = reset_ms as u64 / 1000;
    let start = reset_s.checked_sub(dur_secs)?;
    if now <= start || now >= reset_s {
        return None; // window not started or already rolling over
    }
    let elapsed = (now - start) as f64;
    if elapsed / (dur_secs as f64) < 0.10 {
        return None; // too early in window: flat-rate ETA unreliable
    }
    let rate = used / elapsed; // $/sec
    if rate <= 0.0 || used >= cap {
        return None;
    }
    let secs_to_cap = (cap - used) / rate;
    if secs_to_cap >= (reset_s - now) as f64 {
        return None; // won't hit cap before reset
    }
    Some(secs_to_cap)
}

// ---- plan-based model gating (tables from gating.json) ----
// Mirrors the opencode plugin's evaluateModelAccess; conformance vectors pin
// the two ports together. Unknown plan/model default to allowed — the API
// enforces the real gate.

fn strip_date(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() > 9 {
        let sep = b[b.len() - 9];
        if (sep == b'-' || sep == b'@') && b[b.len() - 8..].iter().all(u8::is_ascii_digit) {
            return s[..s.len() - 9].to_string();
        }
    }
    s.to_string()
}

/// Exact known id (case-insensitive), else alias target, else date-stripped.
pub fn canonical_model(model: &str) -> String {
    let lower = model.to_lowercase();
    if let Some(k) = GATE_KNOWN.iter().find(|m| m.to_lowercase() == lower) {
        return (*k).to_string();
    }
    if let Some((_, to)) = GATE_ALIASES.iter().find(|(from, _)| *from == lower) {
        return GATE_KNOWN
            .iter()
            .find(|m| m.to_lowercase() == to.to_lowercase())
            .map(|m| (*m).to_string())
            .unwrap_or_else(|| model.to_string());
    }
    let stripped = strip_date(model).to_lowercase();
    GATE_KNOWN
        .iter()
        .find(|m| m.to_lowercase() == stripped)
        .map(|m| (*m).to_string())
        .unwrap_or_else(|| model.to_string())
}

/// Table category, else the longest known id that prefixes it (a version bump
/// of a known model inherits its category; otherwise None).
fn model_category(model: &str) -> Option<&'static str> {
    let lower = canonical_model(model).to_lowercase();
    if let Some((_, c)) = GATE_CATEGORIES.iter().find(|(m, _)| m.to_lowercase() == lower) {
        return Some(c);
    }
    let mut best: Option<(&str, &str)> = None;
    for (m, c) in GATE_CATEGORIES {
        if lower.starts_with(&m.to_lowercase()) && best.is_none_or(|(bm, _)| m.len() > bm.len()) {
            best = Some((m, c));
        }
    }
    best.map(|(_, c)| c)
}

pub fn gate_allowed(model: &str, plan_id: &str, unlocked: bool) -> bool {
    gate(model, plan_id, unlocked).0
}

/// Bare model id with any provider qualifier stripped: everything after the
/// FIRST colon. blockedModels entries are provider-qualified
/// ("anthropic:claude-opus-5"); we only serve via command-code lanes, so match
/// on the id portion. A model id that itself contains ':' keeps it.
pub fn bare_model(blocked: &str) -> &str {
    blocked.split_once(':').map(|(_, rest)| rest).unwrap_or(blocked)
}

/// Access decision plus a short human reason (for `models --gated --json`).
pub fn gate(model: &str, plan_id: &str, unlocked: bool) -> (bool, &'static str) {
    if unlocked {
        return (true, "credits unlock all models");
    }
    if plan_id.is_empty() {
        return (true, "unknown plan");
    }
    let canonical = canonical_model(model);
    let cl = canonical.to_lowercase();
    if let Some((_, list)) = GATE_HARD_BLOCKED.iter().find(|(p, _)| *p == plan_id) {
        if list.iter().any(|m| m.to_lowercase() == cl) {
            return (false, "blocked for this plan");
        }
    }
    let Some((_, allowed, blocked)) = GATE_PLANS.iter().find(|(p, _, _)| *p == plan_id) else {
        return (true, "plan has no restrictions");
    };
    let Some(cat) = model_category(model) else {
        return (true, "unknown model category");
    };
    if blocked
        .iter()
        .any(|b| bare_model(b).to_lowercase() == cl)
    {
        return (false, "blocked for this plan");
    }
    if !allowed.contains(&cat) {
        return (false, "premium model, plan is open-models-only");
    }
    (true, "allowed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_names() {
        assert_eq!(plan_name("individual-goat"), "GOAT");
        assert_eq!(plan_name("individual-max-20"), "Max 20x");
        assert_eq!(plan_name("individual-max-10"), "Max 10x");
        assert_eq!(plan_name("individual-go"), "Go");
        assert_eq!(plan_name("teams-pro"), "Team Pro");
        assert_eq!(plan_name("individual-provider"), "Provider");
        assert_eq!(plan_name("bogus"), "Free");
    }

    #[test]
    fn monthly_caps() {
        assert_eq!(plan_monthly_cap("individual-goat"), Some(70.0));
        assert_eq!(plan_monthly_cap("individual-go"), Some(10.0));
        assert_eq!(plan_monthly_cap("individual-pro"), Some(80.0));
        assert_eq!(plan_monthly_cap("individual-max-10"), Some(150.0));
        assert_eq!(plan_monthly_cap("individual-max-20"), Some(300.0));
        assert_eq!(plan_monthly_cap("teams-pro"), Some(40.0));
        assert_eq!(plan_monthly_cap("individual-provider"), None);
    }

    #[test]
    fn iso_hour_start_roundtrips() {
        use crate::dates::iso_hour_start;
        for epoch in [1_700_000_000u64, 1_767_225_600, 1_000_000_000] {
            let s = iso_hour_start(epoch);
            assert!(s.contains('T'), "must be a parseable ISO string: {s}");
            let ms = parse_iso_utc(&s).unwrap() as u64;
            assert_eq!(ms, (epoch - epoch % 3600) * 1000);
        }
    }

    #[test]
    fn money_and_compact() {
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(45.689), "$45.69");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(2_300), "2.3K");
        assert_eq!(compact(316_700_000), "316.7M");
    }

    #[test]
    fn rel_time_human() {
        assert_eq!(rel_time(None, Some(0)), "unknown");
        assert_eq!(rel_time(Some(59_000.0), Some(0)), "<1m");
        assert_eq!(rel_time(Some(120_000.0), Some(60)), "1m");
        assert_eq!(rel_time(Some(7_200_000.0), Some(0)), "2h 0m");
        assert_eq!(rel_time(Some(136_800_000.0), Some(0)), "1d 14h");
        // reset already passed
        assert_eq!(rel_time(Some(1_000.0), Some(100)), "resetting…");
        // no clock → raw epoch
        assert_eq!(rel_time(Some(1_000.0), None), "epoch 1");
    }

    #[test]
    fn elapsed_pct_windows() {
        let now = 1_000_000u64;
        let reset = (now as f64 + 2.5 * 3600.0) * 1000.0;
        assert_eq!(elapsed_pct(Some(reset), 5 * 3600, Some(now)), Some(50));
        assert_eq!(elapsed_pct(Some(1.0), 3600, Some(now)), None);
        assert_eq!(elapsed_pct(None, 3600, Some(now)), None);
    }

    #[test]
    fn iso_offsets_convert_to_utc() {
        // same civil instant expressed with explicit offsets == Z epoch.
        let base = parse_iso_utc("2026-09-27T12:00:00.000Z").unwrap();
        assert_eq!(parse_iso_utc("2026-09-27T12:00:00+00:00").unwrap(), base);
        assert_eq!(parse_iso_utc("2026-09-27T07:00:00-05:00").unwrap(), base);
        assert_eq!(parse_iso_utc("2026-09-27T19:30:00+07:30").unwrap(), base);
        assert_eq!(parse_iso_utc("2026-09-27T07:00:00-0500").unwrap(), base);
        // bad offsets → None
        assert!(parse_iso_utc("2026-09-27T12:00:00X").is_none());
    }

    #[test]
    fn pace_warns_only_after_10pct_elapsed() {
        let now = 1_000_000u64;
        let d = 5 * 3600;
        // 5% elapsed, spend rate would hit cap → suppressed (too early).
        let start5 = now - d / 20;
        let early = pace_eta(Some((start5 + d) as f64 * 1000.0), d, 5.0, 10.0, now);
        assert!(early.is_none(), "must not warn at 5% elapsed: {early:?}");
        // 10% elapsed, same spend rate → pace warning shown.
        let start10 = now - d / 10;
        let at10 = pace_eta(Some((start10 + d) as f64 * 1000.0), d, 5.0, 10.0, now);
        assert_eq!(at10, Some(1800.0));
        // won't hit cap before reset → suppressed.
        let start50 = now - d / 2;
        let fine = pace_eta(Some((start50 + d) as f64 * 1000.0), d, 1.0, 10.0, now);
        assert!(fine.is_none());
        // already over cap → no negative ETA, no warning next to LIMIT EXCEEDED.
        let over = pace_eta(Some((start50 + d) as f64 * 1000.0), d, 12.0, 10.0, now);
        assert!(over.is_none(), "over-cap window must not project: {over:?}");
    }

    /// Shared vectors (conformance.json) that the opencode TypeScript port must
    /// also satisfy — keeps the two implementations from drifting.
    #[test]
    fn conformance_vectors() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../conformance.json")).unwrap();
        for c in v["money"].as_array().unwrap() {
            assert_eq!(money(c["in"].as_f64().unwrap()), c["out"].as_str().unwrap());
        }
        for c in v["compact"].as_array().unwrap() {
            assert_eq!(compact(c["in"].as_u64().unwrap()), c["out"].as_str().unwrap());
        }
        for c in v["pct"].as_array().unwrap() {
            assert_eq!(
                pct(c["used"].as_f64().unwrap(), c["cap"].as_f64().unwrap()),
                c["out"].as_str().unwrap(),
                "pct {}/{}",
                c["used"],
                c["cap"]
            );
        }
        for c in v["bareModel"].as_array().unwrap() {
            assert_eq!(
                bare_model(c["in"].as_str().unwrap()),
                c["out"].as_str().unwrap(),
                "bareModel {}",
                c["in"]
            );
        }
        for c in v["canonicalize"].as_array().unwrap() {
            assert_eq!(
                canonical_model(c["in"].as_str().unwrap()),
                c["out"].as_str().unwrap(),
                "canonicalize {}",
                c["in"]
            );
        }
        for c in v["relTime"].as_array().unwrap() {
            let reset = if c["resetAtMs"].is_null() {
                None
            } else {
                Some(c["resetAtMs"].as_f64().unwrap())
            };
            assert_eq!(rel_time(reset, Some(c["now"].as_u64().unwrap())), c["out"].as_str().unwrap());
        }
        for c in v["parseIso"].as_array().unwrap() {
            let got = parse_iso_utc(c["in"].as_str().unwrap());
            let want = if c["outMs"].is_null() { None } else { Some(c["outMs"].as_f64().unwrap()) };
            assert_eq!(got, want, "parse {}", c["in"]);
        }
        for c in v["plan"].as_array().unwrap() {
            let id = c["id"].as_str().unwrap();
            assert_eq!(plan_name(id), c["name"].as_str().unwrap(), "name {id}");
            assert_eq!(plan_monthly_cap(id), c["cap"].as_f64(), "cap {id}");
        }
        for c in v["gating"].as_array().unwrap() {
            let model = c["model"].as_str().unwrap();
            let plan = c["plan"].as_str().unwrap();
            let unlocked = c["unlocked"].as_bool().unwrap();
            assert_eq!(
                gate_allowed(model, plan, unlocked),
                c["allowed"].as_bool().unwrap(),
                "gate {model} / {plan} / unlocked={unlocked}"
            );
        }
    }
}
