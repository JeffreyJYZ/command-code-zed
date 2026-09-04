use crate::api::{CreditsResp, SubData, UsageSummary};

pub fn plan_name(plan_id: &str) -> &str {
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

/// Monthly credit allocation per plan (docs: pricing-limits).
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

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";

fn color_for(pct: f64) -> &'static str {
    if pct >= 90.0 {
        RED
    } else if pct >= 70.0 {
        YELLOW
    } else {
        GREEN
    }
}

fn money(v: f64) -> String {
    format!("${v:.2}")
}

fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

pub fn bar(used: f64, cap: f64, width: usize) -> String {
    let pct = if cap > 0.0 { (used / cap).clamp(0.0, 1.0) } else { 0.0 };
    let filled = (pct * width as f64).round() as usize;
    let pct_val = if cap > 0.0 { used / cap * 100.0 } else { 0.0 };
    format!(
        "{}{pct_val:>5.1}%{} {}{}{}{}",
        color_for(pct_val),
        RESET,
        GREEN,
        "━".repeat(filled),
        DIM,
        "╱".repeat(width - filled)
    )
}

fn rel_time(reset_at: Option<f64>, now: u64) -> String {
    let Some(ms) = reset_at else {
        return "unknown".into();
    };
    let diff = (ms as u64 / 1000).saturating_sub(now);
    if diff == 0 {
        return "now".into();
    }
    let d = diff / 86400;
    let h = (diff % 86400) / 3600;
    let m = (diff % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// Parse "2026-09-27T12:23:00.000Z" → epoch ms.
fn parse_iso_utc(s: &str) -> Option<f64> {
    let (date, rest) = s.split_once('T')?;
    let rest = rest.trim_end_matches('Z');
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;
    // days since epoch via civil-from-days algorithm (Howard Hinnant)
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400;
    let mut hp = rest.split(':');
    let h: i64 = hp.next().unwrap_or("0").parse().ok()?;
    let mi: i64 = hp.next().unwrap_or("0").parse().ok()?;
    let sec: f64 = hp.next().unwrap_or("0").parse().ok()?;
    Some((secs + h * 3600 + mi * 60) as f64 * 1000.0 + sec * 1000.0)
}

/// Elapsed % of a rolling window: window length = dur_secs, ends at reset_at.
fn elapsed_pct(reset_at: Option<f64>, dur_secs: u64, now: u64) -> Option<u8> {
    let reset_ms = reset_at?;
    let reset_s = reset_ms as u64 / 1000;
    let start = reset_s.checked_sub(dur_secs)?;
    if now < start {
        return None; // window hasn't started
    }
    let elapsed = now - start;
    let pct = (elapsed as f64 / dur_secs as f64 * 100.0).clamp(0.0, 100.0);
    Some(pct.round() as u8)
}

fn window_line(
    label: &str,
    w: &crate::api::Window,
    now: u64,
    bar_width: usize,
    dur_secs: Option<u64>,
) -> String {
    let flag = if w.exceeded { format!(" {RED}{BOLD}LIMIT EXCEEDED{RESET}") } else { String::new() };
    let thru = dur_secs
        .and_then(|d| elapsed_pct(w.reset_at, d, now))
        .map(|p| format!(" · {DIM}window {p}% elapsed{RESET}"))
        .unwrap_or_default();
    format!(
        " {BOLD}{label:<8}{RESET} {} {DIM}{} / {} · resets in {}{thru}{RESET}{flag}",
        bar(w.used, w.cap, bar_width),
        money(w.used),
        money(w.cap),
        rel_time(w.reset_at, now),
    )
}

pub struct Snapshot {
    pub sub: SubData,
    pub credits: CreditsResp,
    pub summary: UsageSummary,
    pub now: u64,
    pub err: Option<String>,
}

pub fn render(s: &Snapshot, bar_width: usize) -> String {
    let mut o = String::new();
    o.push_str(&format!(
        "{BOLD}Command Code Usage{RESET} {DIM}· {} · {}{RESET}\n",
        plan_name(&s.sub.plan_id),
        s.sub.status
    ));
    if let Some(end) = &s.sub.current_period_end {
        o.push_str(&format!(
            "{DIM}Period ends {}{RESET}\n",
            &end[..10.min(end.len())]
        ));
    }
    o.push('\n');

    let monthly_cap = plan_monthly_cap(&s.sub.plan_id);
    match monthly_cap {
        Some(cap) => {
            o.push_str(&format!(
                "{BOLD}Credits{RESET} {} / {} monthly · {} purchased · {} free\n",
                money(s.credits.credits.monthly_credits),
                money(cap),
                money(s.credits.credits.purchased_credits),
                money(s.credits.credits.free_credits),
            ));
        }
        None => {
            o.push_str(&format!(
                "{BOLD}Credits{RESET} {} monthly · {} purchased · {} free\n",
                money(s.credits.credits.monthly_credits),
                money(s.credits.credits.purchased_credits),
                money(s.credits.credits.free_credits),
            ));
        }
    }

    if let Some(e) = &s.err {
        o.push_str(&format!("\n{RED}error:{RESET} {e}\n"));
    }

    o.push_str(&format!("\n{BOLD}Usage windows{RESET}\n"));
    // Monthly: cap from plan table, used = cap - remaining monthly credits.
    // ponytail: reset_at parsed from ISO date has no UTC offset; treated as UTC — off by hours at most.
    if let Some(cap) = monthly_cap {
        let used = (cap - s.credits.credits.monthly_credits).clamp(0.0, cap);
        let reset_at = s.sub.current_period_end.as_ref().and_then(|e| parse_iso_utc(e));
        let dur = match (&s.sub.current_period_start, &s.sub.current_period_end) {
            (Some(st), Some(en)) => parse_iso_utc(st)
                .zip(parse_iso_utc(en))
                .map(|(a, b)| ((b - a) as u64 / 1000).max(1)),
            _ => None,
        };
        o.push_str(&window_line("Monthly", &crate::api::Window {
            used,
            cap,
            exceeded: false,
            reset_at,
        }, s.now, bar_width, dur));
        o.push('\n');
    }
    match (&s.credits.window_limits.five_hour, &s.credits.window_limits.weekly) {
        (Some(h5), Some(wk)) => {
            o.push_str(&window_line("5-hour", h5, s.now, bar_width, Some(5 * 3600)));
            o.push('\n');
            o.push_str(&window_line("Weekly", wk, s.now, bar_width, Some(7 * 86400)));
            o.push('\n');
        }
        (None, None) => {
            o.push_str(&format!(" {DIM}none on this plan (pay-as-you-go){RESET}\n"));
        }
        (h5, wk) => {
            if let Some(w) = h5 {
                o.push_str(&window_line("5-hour", w, s.now, bar_width, Some(5 * 3600)));
                o.push('\n');
            }
            if let Some(w) = wk {
                o.push_str(&window_line("Weekly", w, s.now, bar_width, Some(7 * 86400)));
                o.push('\n');
            }
        }
    }

    o.push_str(&format!("\n{BOLD}This billing period{RESET}\n"));
    o.push_str(&format!(
        " Requests {CYAN}{}{RESET} · Cost {CYAN}{}{RESET} · Tokens {CYAN}{}{RESET} in / {CYAN}{}{RESET} out · Success {CYAN}{:.0}%{RESET}\n",
        compact(s.summary.total_count),
        money(s.summary.total_cost),
        compact(s.summary.total_tokens_in),
        compact(s.summary.total_tokens_out),
        s.summary.success_rate,
    ));

    o
}

/// One-shot plain output (no ANSI colors), for scripts.
pub fn render_plain(s: &Snapshot, bar_width: usize) -> String {
    let mut o = String::new();
    o.push_str(&format!(
        "Command Code Usage · {} · {}\n",
        plan_name(&s.sub.plan_id),
        s.sub.status
    ));
    o.push_str(&format!(
        "Credits: {} monthly, {} purchased, {} free\n",
        money(s.credits.credits.monthly_credits),
        money(s.credits.credits.purchased_credits),
        money(s.credits.credits.free_credits),
    ));
    if let Some(w) = &s.credits.window_limits.five_hour {
        o.push_str(&format!(
            "5-hour: {:.0}% ({} / {}) · resets in {}\n",
            if w.cap > 0.0 { w.used / w.cap * 100.0 } else { 0.0 },
            money(w.used),
            money(w.cap),
            rel_time(w.reset_at, s.now),
        ));
    }
    if let Some(w) = &s.credits.window_limits.weekly {
        o.push_str(&format!(
            "Weekly: {:.0}% ({} / {}) · resets in {}\n",
            if w.cap > 0.0 { w.used / w.cap * 100.0 } else { 0.0 },
            money(w.used),
            money(w.cap),
            rel_time(w.reset_at, s.now),
        ));
    }
    o.push_str(&format!(
        "Period: {} requests, {}, {} in/{} out tokens\n",
        s.summary.total_count,
        money(s.summary.total_cost),
        compact(s.summary.total_tokens_in),
        compact(s.summary.total_tokens_out),
    ));
    let _ = bar_width;
    o
}
