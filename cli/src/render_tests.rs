use super::render::*;
use crate::render::{Snapshot};

// Plan-name/cap, money, compact, rel_time, and ISO cases are covered by the
// shared conformance vectors in core (conformance.json) — only presentation
// (bars, window lines, tables, templates) is asserted here.

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
fn pace_warns_only_after_10pct_elapsed() {
    use crate::api::Window;
    let now = 1_000_000u64;
    let d = 5 * 3600;

    // 5% elapsed, spend rate would hit cap → suppressed (too early).
    let start5 = now - d / 20;
    let w_early = Window {
        used: 5.0,
        cap: 10.0,
        exceeded: false,
        reset_at: Some((start5 + d) as f64 * 1000.0),
    };
    let line = window_line("5-hour", &w_early, now, 20, Some(d));
    assert!(!line.contains("on pace"), "must not warn at 5% elapsed: {line}");

    // 10% elapsed, same spend rate → pace warning shown.
    let start10 = now - d / 10;
    let w_at = Window {
        used: 5.0,
        cap: 10.0,
        exceeded: false,
        reset_at: Some((start10 + d) as f64 * 1000.0),
    };
    let line = window_line("5-hour", &w_at, now, 20, Some(d));
    assert!(line.contains("on pace"), "should warn at 10% elapsed: {line}");
}

#[test]
fn plans_table_marks_current_by_exact_name() {
    let out = plans_table("individual-goat", false);
    assert!(out.contains("*GOAT"));
    assert!(!out.contains("*Go "), "substring 'go' must not mark Go: {out}");
    let out = plans_table("unknown-plan", false);
    assert!(!out.contains('*'), "no current plan → no mark: {out}");
    // colors=true bolds the current row
    let out = plans_table("individual-goat", true);
    assert!(out.contains("\x1b[1m*GOAT"));
}

#[test]
fn plans_json_marks_current() {
    let out = plans_json("individual-goat");
    assert!(out.starts_with('[') && out.ends_with(']'));
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    let goat = arr.iter().find(|p| p["name"] == "GOAT").unwrap();
    assert_eq!(goat["price"], "$10");
    assert_eq!(goat["creditsMonthly"], "$70");
    assert_eq!(goat["fiveHour"], "$14");
    assert_eq!(goat["weekly"], "$35");
    assert_eq!(goat["current"], true);
    let go = arr.iter().find(|p| p["name"] == "Go").unwrap();
    assert_eq!(go["price"], "$1");
    assert_eq!(go["current"], false);
}

#[test]
fn render_json_is_valid_and_complete() {
    let s = snapshot_fixture();
    let out = render_json(&s);
    assert!(out.starts_with('{') && out.ends_with('}'));
    assert!(out.contains("\"plan\":\"GOAT\""));
    assert!(out.contains("\"fiveHour\":{"));
    assert!(out.contains("\"error\":null"));
    // parse round-trip
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["plan"], "GOAT");
    assert_eq!(v["monthlyCap"], 70.0);
}

#[test]
fn plain_render_contains_sections() {
    let s = snapshot_fixture();
    let out = render_plain(&s);
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
    let out_plain = render_plain(&s);
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
        five_hour_eta: None,
        weekly_eta: None,
        bar_width: 10,
        colors: tpl_colors,
        ascii,
    };

    // full template
    let d0 = d(true, false);
    let out = render_statusline("{plan} {credits}/{cap} · 5h {5h_bar} · wk {wk_bar}", &d0);
    assert!(out.contains("\x1b["));
    let plain = d(false, false);
    let out = render_statusline("{plan} {credits}/{cap} · 5h {5h_bar} · wk {wk_bar}", &plain);
    assert!(out.contains("GOAT"));
    assert!(out.contains("$59.30/$70.00"));
    assert!(out.contains("5h"));
    assert!(out.contains("wk"));
    assert!(!out.contains('\x1b'));

    // minimal: plan only
    let out = render_statusline("{plan}", &d0);
    assert_eq!(out, "GOAT");

    // pct placeholders
    let out = render_statusline("{5h_pct}|{wk_pct}", &d0);
    assert_eq!(out, "20%|10%");

    // used/cap
    let out = render_statusline("{5h_used} of {5h_cap}", &d0);
    assert_eq!(out, "$2.79 of $14.00");

    // credits_bar shows used % of monthly cap
    let out = render_statusline("{credits_bar}", &d0);
    assert!(out.contains("15.3%"));

    // unknown placeholders dropped
    let out = render_statusline("{plan} {bogus} end", &d0);
    assert_eq!(out, "GOAT  end");

    // unclosed brace passes through verbatim
    let out = render_statusline("{plan} {oops", &d0);
    assert_eq!(out, "GOAT {oops");

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

    // eta placeholders render when set, empty otherwise
    assert_eq!(render_statusline("{5h_eta}", &d0), "");
    let deta = StatusData {
        plan: "GOAT",
        monthly_remaining: 1.0,
        monthly_cap: 70.0,
        five_hour: &h5,
        weekly: &wk,
        five_hour_eta: Some("on pace to hit cap in 1h 2m".into()),
        weekly_eta: None,
        bar_width: 10,
        colors: false,
        ascii: false,
    };
    assert_eq!(render_statusline("{5h_eta}|{wk_eta}", &deta), "on pace to hit cap in 1h 2m|");

    // zero-cap plan: bars show 0%, no divide-by-zero
    let h5z: Option<(f64, f64)> = None;
    let dz = StatusData {
        plan: "Free",
        monthly_remaining: 0.0,
        monthly_cap: 0.0,
        five_hour: &h5z,
        weekly: &h5z,
        five_hour_eta: None,
        weekly_eta: None,
        bar_width: 10,
        colors: true,
        ascii: false,
    };
    let out = render_statusline("{plan} {5h_pct} {wk_pct} {credits_bar}", &dz);
    assert!(out.contains("0%"));
}

#[test]
fn sparkline_small_deltas_visible() {
    // tiny recent delta next to a big old spike must not render as zero bar
    let out = sparkline(&[0.5, 0.0, 0.02]);
    assert_ne!(out.chars().last(), Some('▁'));
    // zero deltas still render as zero bar
    let out = sparkline(&[0.5, 0.0, 0.0]);
    assert_eq!(out.chars().last(), Some('▁'));
}

#[test]
fn sparkline_shapes() {
    // flat → all lowest bar
    assert_eq!(sparkline(&[1.0, 1.0, 1.0]), "███"); // flat values scale to full-height max
    // rising trend
    assert_eq!(sparkline(&[0.0, 5.0, 10.0]), "▁▅█"); // 0/10→▁, 5/10→mid, 10/10→full
    // fewer than 2 points → empty
    assert_eq!(sparkline(&[]), "");
    assert_eq!(sparkline(&[5.0]), "");
    // all zeros → all low bars, no NaN panic
    assert_eq!(sparkline(&[0.0, 0.0]), "▁▁");
}
