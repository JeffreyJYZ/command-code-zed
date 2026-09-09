use crate::api::{CreditsResp, SubData, UsageSummary};
pub use cmduse_core::{compact, money, plan_monthly_cap, plan_name};

pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD: &str = "\x1b[1m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const CYAN: &str = "\x1b[36m";

fn color_for(pct: f64) -> &'static str {
    if pct >= 90.0 {
        RED
    } else if pct >= 70.0 {
        YELLOW
    } else {
        GREEN
    }
}

pub fn bar(used: f64, cap: f64, width: usize) -> String {
    let pct = if cap > 0.0 { (used / cap).clamp(0.0, 1.0) } else { 0.0 };
    let filled = (pct * width as f64).round() as usize;
    let pct_val = pct * 100.0;
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

pub fn rel_time(reset_at: Option<f64>, now: u64) -> String {
    cmduse_core::rel_time(reset_at, Some(now))
}

/// date/ISO helpers live in cmduse-core; re-export here (tests + render use it)
pub use cmduse_core::parse_iso_utc;

/// Elapsed % of a rolling window: window length = dur_secs, ends at reset_at.
pub fn elapsed_pct(reset_at: Option<f64>, dur_secs: u64, now: u64) -> Option<u8> {
    cmduse_core::elapsed_pct(reset_at, dur_secs, Some(now))
}

pub fn window_line(
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
    // burn-rate projection: spend rate over window elapsed time → when cap hits.
    // ponytail: assumes flat spend rate; bursty sessions shift the ETA.
    // upgrade: revisit if ETA misfires in practice (no server rate history yet).
    let pace = dur_secs
        .and_then(|d| {
            cmduse_core::pace_eta(w.reset_at, d, w.used, w.cap, now)
                .map(|secs| rel_time(Some(secs * 1000.0), now))
        })
        .map(|eta| format!(" · {YELLOW}on pace to hit cap in {eta}{RESET}"))
        .unwrap_or_default();
    format!(
        " {BOLD}{label:<8}{RESET} {} {DIM}{} / {} · resets in {}{thru}{pace}{RESET}{flag}",
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
    // error: single minimal frame. Don't render fake zero-data plan/credits
    // below it (was: "Free · —" with $0.00 everywhere on API failure).
    if let Some(e) = &s.err {
        return format!(
            "{BOLD}Command Code Usage{RESET} {DIM}· fetch failed{RESET}\n{RED}error:{RESET} {e}\n{DIM}retrying on next refresh{RESET}"
        );
    }
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

    o.push_str(&format!("\n{BOLD}Usage windows{RESET}\n"));
    // Monthly: cap from plan table, used = cap - remaining monthly credits.
    // ponytail: reset_at parsed from ISO date has no UTC offset; treated as UTC — off by hours at most.
    // upgrade: parse an explicit offset if the API ever emits one (all-Z today).
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

/// ASCII cost trend: 8-point sparkline, last `cap` samples of $ spent.
pub fn sparkline(history: &[f64]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if history.len() < 2 {
        return String::new();
    }
    let max = history.iter().cloned().fold(0.0_f64, f64::max);
    if max <= 0.0 {
        return "▁".repeat(history.len());
    }
    // any nonzero activity gets a visible bar; otherwise a 0.02 delta next
    // to an old 0.50 spike rounds to zero and looks dead
    history
        .iter()
        .map(|v| {
            let idx = (v / max * 7.0).round() as usize;
            BARS[idx.max(if *v > 0.0 { 1 } else { 0 })]
        })
        .collect()
}
