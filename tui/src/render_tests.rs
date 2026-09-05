use super::render::*;
use crate::render::{Snapshot};

#[test]
fn plan_names_all_plans() {
    assert_eq!(plan_name("individual-goat"), "GOAT");
    assert_eq!(plan_name("individual-go"), "Go");
    assert_eq!(plan_name("individual-pro"), "Pro");
    assert_eq!(plan_name("individual-pro-v1"), "Pro");
    assert_eq!(plan_name("individual-max"), "Max 10x");
    assert_eq!(plan_name("individual-max-20"), "Max 20x");
    assert_eq!(plan_name("individual-max-10x"), "Max 10x");
    assert_eq!(plan_name("teams-pro"), "Team Pro");
    assert_eq!(plan_name("individual-provider"), "Provider");
    assert_eq!(plan_name("enterprise-custom"), "Enterprise");
    assert_eq!(plan_name("whatever-unknown"), "Free");
    assert_eq!(plan_name(""), "Free");
}

#[test]
fn monthly_caps_match_docs() {
    // from commandcode.ai pricing-limits
    assert_eq!(plan_monthly_cap("individual-goat"), Some(70.0));
    assert_eq!(plan_monthly_cap("individual-go"), Some(10.0));
    assert_eq!(plan_monthly_cap("individual-pro"), Some(80.0));
    assert_eq!(plan_monthly_cap("individual-max-20"), Some(300.0));
    assert_eq!(plan_monthly_cap("individual-max-10x"), Some(150.0));
    assert_eq!(plan_monthly_cap("teams-pro"), Some(40.0));
    // no fixed pool
    assert_eq!(plan_monthly_cap("individual-provider"), None);
    assert_eq!(plan_monthly_cap("enterprise-x"), None);
    assert_eq!(plan_monthly_cap("free"), None);
}

#[test]
fn max_plan_id_ordering_matters() {
    // "max-20" contains "max"; ensure 20 check wins
    assert_eq!(plan_monthly_cap("individual-max-20x"), Some(300.0));
    assert_eq!(plan_monthly_cap("max-20-special"), Some(300.0));
}

#[test]
fn bar_boundaries() {
    // bar() embeds a colored % label; assert fill/empty chars
    let fill = |used: f64, cap: f64, w: usize| {
        let b = bar(used, cap, w);
        b.chars().filter(|c| *c == '━').count()
    };
    let empty = |used: f64, cap: f64, w: usize| {
        let b = bar(used, cap, w);
        b.chars().filter(|c| *c == '╱').count()
    };
    assert_eq!(fill(0.0, 10.0, 20), 0);
    assert_eq!(empty(0.0, 10.0, 20), 20);
    assert_eq!(fill(10.0, 10.0, 20), 20);
    assert_eq!(fill(0.0, 0.0, 20), 0); // no divide by zero
    assert_eq!(fill(50.0, 10.0, 20), 20); // clamp over-usage
    assert_eq!(fill(5.0, 10.0, 20), 10); // half fill rounds to 10
    assert_eq!(empty(5.0, 10.0, 20), 10);
    assert_eq!(fill(5.0, 10.0, 5), 3); // rounds to 3 of 5
    assert_eq!(empty(5.0, 10.0, 5), 2);
    assert!(bar(7.5, 10.0, 20).contains("75.0%"));
    assert!(bar(95.0, 10.0, 20).contains("100.0%")); // clamps at cap
}

#[test]
fn money_formatting() {
    assert_eq!(money(0.0), "$0.00");
    assert_eq!(money(9.25), "$9.25");
    assert_eq!(money(70.0), "$70.00");
    assert_eq!(money(0.005), "$0.01"); // rounds
}

#[test]
fn compact_formatting() {
    assert_eq!(compact(0), "0");
    assert_eq!(compact(999), "999");
    assert_eq!(compact(1000), "1.0K");
    assert_eq!(compact(28_129_791), "28.1M");
    assert_eq!(compact(64_096_255), "64.1M");
}

#[test]
fn rel_time_windows() {
    let now = 1_000_000u64;
    // 2 minutes (reset_at in ms)
    assert_eq!(rel_time(Some((now as f64 + 120.0) * 1000.0), now), "2m");
    // 1h 5m
    assert_eq!(rel_time(Some((now as f64 + 3900.0) * 1000.0), now), "1h 5m");
    // 2d 3h
    assert_eq!(rel_time(Some((now as f64 + 2.0 * 86400.0 + 3.0 * 3600.0) * 1000.0), now), "2d 3h");
    // already passed (clock skew / rolling over)
    assert_eq!(rel_time(Some((now as f64 - 1.0) * 1000.0), now), "resetting…");
    // exactly now
    assert_eq!(rel_time(Some(now as f64 * 1000.0), now), "resetting…");
    // under a minute
    assert_eq!(rel_time(Some((now as f64 + 30.0) * 1000.0), now), "<1m");
    // missing
    assert_eq!(rel_time(None, now), "unknown");
}

#[test]
fn iso_utc_parsing() {
    // 2026-09-27T12:23:00.000Z → known epoch
    let ms = parse_iso_utc("2026-09-27T12:23:00.000Z").unwrap();
    // 2026-09-27 12:23:00 UTC = 1789592580
    assert_eq!(ms as u64 / 1000, 1_790_511_780);
    // without millis
    assert_eq!(parse_iso_utc("2026-01-01T00:00:00Z").unwrap() as u64 / 1000, 1_767_225_600);
    // leap year Feb 29 2024
    assert_eq!(parse_iso_utc("2024-02-29T12:00:00Z").unwrap() as u64 / 1000, 1_709_208_000);
    // garbage
    assert!(parse_iso_utc("not-a-date").is_none());
    assert!(parse_iso_utc("").is_none());
    assert!(parse_iso_utc("2026-13-45T99:99:99Z").is_some()); // lenient, no validation needed
}

#[test]
fn elapsed_pct_windows() {
    let now = 1_000_000u64;
    // window ends now + 2.5h, duration 5h → 50% elapsed
    let reset = (now as f64 + 2.5 * 3600.0) * 1000.0;
    assert_eq!(elapsed_pct(Some(reset), 5 * 3600, now), Some(50));
    // 0% at window start
    let reset0 = (now as f64 + 5.0 * 3600.0) * 1000.0;
    assert_eq!(elapsed_pct(Some(reset0), 5 * 3600, now), Some(0));
    // 100% at window end
    let reset100 = now as f64 * 1000.0;
    assert_eq!(elapsed_pct(Some(reset100), 5 * 3600, now), Some(100));
    // reset in the past but window start still valid → 100%
    let stale = (now as f64 - 1000.0) * 1000.0; // ended ~17min ago, 5h window
    assert_eq!(elapsed_pct(Some(stale), 5 * 3600, now), Some(100));
    // window start before epoch → None (underflow guard)
    assert_eq!(elapsed_pct(Some(1.0), 5 * 3600, now), None);
    // no reset info
    assert_eq!(elapsed_pct(None, 3600, now), None);
    // rounding: 1/3 of window = 33
    let reset3 = (now as f64 + 2.0 * 3600.0) * 1000.0;
    assert_eq!(elapsed_pct(Some(reset3), 3 * 3600, now), Some(33));
}

#[test]
fn window_line_includes_elapsed_and_flag() {
    use crate::api::Window;
    let now = 1_000_000u64;
    let w = Window {
        used: 5.0,
        cap: 10.0,
        exceeded: false,
        reset_at: Some((now as f64 + 2.5 * 3600.0) * 1000.0),
    };
    let line = window_line("5-hour", &w, now, 20, Some(5 * 3600));
    assert!(line.contains("50%"));
    assert!(line.contains("$5.00 / $10.00"));
    assert!(line.contains("resets in"));

    let w_exceeded = Window { used: 15.0, cap: 10.0, exceeded: true, reset_at: None };
    let line = window_line("Weekly", &w_exceeded, now, 20, Some(7 * 86400));
    assert!(line.contains("LIMIT EXCEEDED"));

    // no duration → no elapsed suffix
    let line = window_line("Monthly", &w, now, 20, None);
    assert!(!line.contains("elapsed"));
}

#[test]
fn plain_render_contains_sections() {
    let s = snapshot_fixture();
    let out = render_plain(&s, 20);
    assert!(out.contains("Command Code Usage"));
    assert!(out.contains("GOAT"));
    assert!(out.contains("Credits:"));
    assert!(out.contains("5-hour:"));
    assert!(out.contains("Weekly:"));
    assert!(out.contains("Period:"));
}

#[test]
fn ansi_render_contains_sections_and_quota() {
    let s = snapshot_fixture();
    let out = render(&s, 20);
    assert!(out.contains("Command Code Usage"));
    assert!(out.contains("GOAT"));
    assert!(out.contains("$61.44 / $70.00 monthly"));
    assert!(out.contains("Usage windows"));
    assert!(out.contains("Monthly"));
    assert!(out.contains("This billing period"));
    assert!(out.contains("Requests"));
    // error rendering
    let mut err_s = snapshot_fixture();
    err_s.err = Some("boom".into());
    let out = render(&err_s, 20);
    assert!(out.contains("error:"));
    assert!(out.contains("boom"));
}

#[test]
fn render_free_plan_no_monthly_cap() {
    let mut s = snapshot_fixture();
    s.sub.plan_id = "free".into();
    let out = render(&s, 20);
    assert!(!out.contains("/ $70.00"));
    assert!(out.contains("$61.44 monthly")); // remaining shown, no quota
    let out_plain = render_plain(&s, 20);
    assert!(out_plain.contains("$61.44 monthly"));
}

fn snapshot_fixture() -> Snapshot {
    Snapshot {
        sub: crate::api::SubData {
            status: "active".into(),
            plan_id: "individual-goat".into(),
            current_period_start: Some("2026-08-27T12:23:00.000Z".into()),
            current_period_end: Some("2026-09-27T12:23:00.000Z".into()),
        },
        credits: crate::api::CreditsResp {
            credits: crate::api::Credits {
                monthly_credits: 61.44,
                purchased_credits: 0.0,
                free_credits: 0.0,
            },
            window_limits: crate::api::WindowLimits {
                five_hour: Some(crate::api::Window {
                    used: 2.79,
                    cap: 14.0,
                    exceeded: false,
                    reset_at: Some(1_789_368_000_000.0),
                }),
                weekly: Some(crate::api::Window {
                    used: 2.79,
                    cap: 35.0,
                    exceeded: false,
                    reset_at: Some(1_789_912_000_000.0),
                }),
            },
        },
        summary: crate::api::UsageSummary {
            total_count: 733,
            total_cost: 9.25,
            success_rate: 100.0,
            total_tokens_in: 64_096_255,
            total_tokens_out: 236_706,
        },
        now: 1_789_368_000_000 / 1000,
        err: None,
    }
}

#[test]
fn statusline_templates() {
    use crate::report_render::{render_statusline, StatusData};
    let h5 = Some((2.79, 14.0));
    let wk = Some((3.61, 35.0));
    let d = |tpl_colors: bool, ascii: bool| StatusData {
        plan: "GOAT",
        monthly_remaining: 59.3,
        monthly_cap: 70.0,
        five_hour: &h5,
        weekly: &wk,
        bar_width: 10,
        colors: tpl_colors,
        ascii,
    };

    // full template
    let d0 = d(true, false);
    let out = render_statusline("{plan} {credits}/{cap} · 5h {5h_bar} · wk {wk_bar}", &d0);
    let plain = crate::report_render::strip_ansi(&out);
    assert!(plain.contains("GOAT"));
    assert!(plain.contains("$59.30/$70.00"));
    assert!(plain.contains("5h"));
    assert!(plain.contains("wk"));

    // minimal: plan only
    let out = render_statusline("{plan}", &d0);
    assert_eq!(out, "GOAT");

    // pct placeholders
    let out = render_statusline("{5h_pct}|{wk_pct}", &d0);
    let plain = crate::report_render::strip_ansi(&out);
    assert_eq!(plain, "20%|10%");

    // used/cap
    let out = render_statusline("{5h_used} of {5h_cap}", &d0);
    assert_eq!(crate::report_render::strip_ansi(&out), "$2.79 of $14.00");

    // credits_bar shows used % of monthly cap
    let out = render_statusline("{credits_bar}", &d0);
    assert!(out.contains("15.3%"));

    // unknown placeholders dropped
    let out = render_statusline("{plan} {bogus} end", &d0);
    assert_eq!(crate::report_render::strip_ansi(&out), "GOAT  end");

    // unclosed brace passes through verbatim
    let out = render_statusline("{plan} {oops", &d0);
    assert_eq!(crate::report_render::strip_ansi(&out), "GOAT {oops");

    // ascii bars
    let d1 = d(false, true);
    let out = render_statusline("{5h_bar}", &d1);
    assert!(out.contains('#'));
    assert!(out.contains('-'));
    assert!(!out.contains('━'));

    // colors stripped when colors=false
    let d2 = d(false, false);
    let out = render_statusline("{credits}", &d2);
    assert!(!out.contains('\x1b'));
    assert_eq!(out, "$59.30");

    // multi-line templates allowed
    let out = render_statusline("{plan}\n{credits_bar}", &d0);
    assert!(out.contains('\n'));

    // zero-cap plan: bars show 0%, no divide-by-zero
    let h5z: Option<(f64, f64)> = None;
    let dz = StatusData {
        plan: "Free",
        monthly_remaining: 0.0,
        monthly_cap: 0.0,
        five_hour: &h5z,
        weekly: &h5z,
        bar_width: 10,
        colors: true,
        ascii: false,
    };
    let out = render_statusline("{plan} {5h_pct} {wk_pct} {credits_bar}", &dz);
    let plain = crate::report_render::strip_ansi(&out);
    assert!(plain.contains("0%"));
}
