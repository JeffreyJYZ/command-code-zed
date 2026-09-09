pub mod dates;
pub use dates::parse_iso_utc;

/// Plan display name from the API's planId (e.g. "individual-goat" → "GOAT").
pub fn plan_name(plan_id: &str) -> &'static str {
    let id = plan_id.to_lowercase();
    if id.contains("enterprise") {
        "Enterprise"
    } else if id.contains("provider") {
        "Provider"
    } else if id.contains("team") {
        "Team Pro"
    } else if id.contains("max") {
        if id.contains("20") { "Max 20x" } else { "Max 10x" }
    } else if id.contains("goat") {
        "GOAT"
    } else if id.contains("pro") {
        "Pro"
    } else if id.contains("go") {
        "Go"
    } else {
        "Free"
    }
}

/// Monthly credit pool per plan. One shared pool per plan (verified — the
/// docs' per-model allowances are not what the API meters). None = PAYG.
pub fn plan_monthly_cap(plan_id: &str) -> Option<f64> {
    let id = plan_id.to_lowercase();
    if id.contains("enterprise") || id.contains("provider") {
        None
    } else if id.contains("team") {
        Some(40.0)
    } else if id.contains("max") {
        if id.contains("20") { Some(300.0) } else { Some(150.0) }
    } else if id.contains("goat") {
        Some(70.0)
    } else if id.contains("pro") {
        Some(80.0)
    } else if id.contains("go") {
        Some(10.0)
    } else {
        None
    }
}

pub fn money(v: f64) -> String {
    format!("${v:.2}")
}

pub fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

/// Percent string of used/cap: "34%" or "—" when cap unknown.
pub fn pct(used: f64, cap: f64) -> String {
    if cap > 0.0 {
        format!("{:.0}%", (used / cap) * 100.0)
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
    if rate <= 0.0 {
        return None;
    }
    let secs_to_cap = (cap - used) / rate;
    if secs_to_cap >= (reset_s - now) as f64 {
        return None; // won't hit cap before reset
    }
    Some(secs_to_cap)
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
    }
}
