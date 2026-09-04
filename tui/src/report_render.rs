use crate::reports::{ByDay, Totals};

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

pub fn money(v: f64) -> String {
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

fn bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0).clamp(0.0, 1.0) * width as f64).round() as usize;
    let color = if pct >= 90.0 {
        RED
    } else if pct >= 70.0 {
        YELLOW
    } else {
        GREEN
    };
    format!(
        "{color}{pct:>5.1}%{} {}{}{}",
        RESET,
        "━".repeat(filled),
        DIM,
        "╱".repeat(width - filled)
    )
}

/// Account-wide daily table (all harnesses, from usage API)
pub fn account_table(by_day: &crate::reports::ByDay, total: &crate::reports::Totals, json: bool) -> String {
    if json {
        let days: Vec<String> = by_day
            .iter()
            .map(|(d, t)| format!("\"{}\":{}", d, total_json(t)))
            .collect();
        return format!(
            "{{\"scope\":\"account\",\"total\":{},{}}}",
            total_json(total),
            format_args!("\"days\":{{{}}}", days.join(","))
        );
    }
    let mut o = String::new();
    o.push_str(&format!(
        "{BOLD}Account usage{RESET} {DIM}(all harnesses, last recorded days){RESET}\n\n"
    ));
    o.push_str(&format!(
        " {BOLD}{:<12}{RESET} {:>6} {:>10} {:>10} {:>10}\n",
        "Day", "Reqs", "In", "Out", "Cost"
    ));
    for (day, t) in by_day {
        o.push_str(&format!(
            " {:<12} {:>6} {:>10} {:>10} {:>10}\n",
            day,
            compact(t.requests),
            compact(t.usage.input_tokens),
            compact(t.usage.output_tokens),
            money(t.usage.cost_usd),
        ));
    }
    o.push_str(&format!(
        " {DIM}{:<12}{RESET} {:>6} {:>10} {:>10} {:>10}\n",
        "total",
        compact(total.requests),
        compact(total.usage.input_tokens),
        compact(total.usage.output_tokens),
        money(total.usage.cost_usd),
    ));
    o
}

pub fn table(by_day: &ByDay, total: &Totals, days: Option<usize>, json: bool) -> String {
    let mut o = String::new();
    if json {
        o.push_str(&format!(
            "{{\"total\":{},{}}}",
            total_json(total),
            by_day_json(by_day)
        ));
        return o;
    }
    o.push_str(&format!(
        "{BOLD}Local usage{RESET} {DIM}(offline, from ~/.commandcode/projects){RESET}\n\n"
    ));
    o.push_str(&format!(
        " {BOLD}{:<12}{RESET} {:>6} {:>10} {:>10} {:>10} {:>10}\n",
        "Day", "Reqs", "In", "Out", "Cache rd", "Cost"
    ));
    let mut spent_days = 0;
    let last_n = days.unwrap_or(usize::MAX);
    let day_count = by_day.len();
    for (day, t) in by_day.iter().skip(day_count.saturating_sub(last_n)) {
        spent_days += 1;
        o.push_str(&format!(
            " {:<12} {:>6} {:>10} {:>10} {:>10} {:>10}\n",
            day,
            compact(t.requests),
            compact(t.usage.input_tokens),
            compact(t.usage.output_tokens),
            compact(t.usage.cache_read_tokens),
            money(t.usage.cost_usd),
        ));
    }
    if spent_days == 0 {
        o.push_str(" {DIM}no recorded sessions yet{RESET}\n".replace("{DIM}", DIM).as_str());
    }
    o.push_str(&format!(
        " {DIM}{:<12}{RESET} {:>6} {:>10} {:>10} {:>10} {:>10}\n",
        "total",
        compact(total.requests),
        compact(total.usage.input_tokens),
        compact(total.usage.output_tokens),
        compact(total.usage.cache_read_tokens),
        money(total.usage.cost_usd),
    ));
    o
}

pub fn total_json(t: &Totals) -> String {
    format!(
        "{{\"requests\":{},\"tokensIn\":{},\"tokensOut\":{},\"cacheRead\":{},\"cacheWrite\":{},\"costUsd\":{:.4}}}",
        t.requests, t.usage.input_tokens, t.usage.output_tokens, t.usage.cache_read_tokens, t.usage.cache_write_tokens, t.usage.cost_usd
    )
}

fn by_day_json(by_day: &ByDay) -> String {
    let days: Vec<String> = by_day
        .iter()
        .map(|(d, t)| format!("\"{}\":{}", d, total_json(t)))
        .collect();
    format!("\"days\":{{{}}}", days.join(","))
}

/// Sessions grouped per project, like ccusage's --instances
pub fn project_table(by_project: &crate::reports::ByProject, json: bool) -> String {
    if json {
        let items: Vec<String> = by_project
            .iter()
            .map(|(p, t)| format!("\"{}\":{}", p, total_json(t)))
            .collect();
        return format!("{{\"projects\":{{{}}}}}", items.join(","));
    }
    let mut o = format!("\n{BOLD}By project{RESET}\n");
    o.push_str(&format!(
        " {BOLD}{:<32}{RESET} {:>6} {:>10} {:>10}\n",
        "Project", "Reqs", "Tokens", "Cost"
    ));
    for (p, t) in by_project {
        let tokens = t.usage.input_tokens + t.usage.output_tokens + t.usage.cache_read_tokens;
        o.push_str(&format!(
            " {:<32} {:>6} {:>10} {:>10}\n",
            p,
            compact(t.requests),
            compact(tokens),
            money(t.usage.cost_usd),
        ));
    }
    o
}

pub fn model_table(by_model: &crate::reports::ByModel, json: bool) -> String {
    if json {
        let items: Vec<String> = by_model
            .iter()
            .map(|(m, t)| format!("\"{}\":{}", m, total_json(t)))
            .collect();
        return format!("{{\"models\":{{{}}}}}", items.join(","));
    }
    let mut o = format!("\n{BOLD}By model{RESET}\n");
    o.push_str(&format!(
        " {BOLD}{:<32}{RESET} {:>6} {:>10} {:>10}\n",
        "Model", "Reqs", "Tokens", "Cost"
    ));
    let total_cost: f64 = by_model.values().map(|t| t.usage.cost_usd).sum();
    for (m, t) in by_model {
        let tokens = t.usage.input_tokens + t.usage.output_tokens + t.usage.cache_read_tokens;
        let share = if total_cost > 0.0 {
            format!(" {DIM}{:>5.1}%{RESET}", t.usage.cost_usd / total_cost * 100.0)
        } else {
            String::new()
        };
        o.push_str(&format!(
            " {:<32} {:>6} {:>10} {:>10}{share}\n",
            m,
            compact(t.requests),
            compact(tokens),
            money(t.usage.cost_usd),
        ));
    }
    o
}

/// Compact single-line status for shell prompts / tmux.
pub fn statusline(
    plan: &str,
    monthly_remaining: f64,
    monthly_cap: f64,
    h5: Option<(f64, f64)>,
    weekly: Option<(f64, f64)>,
) -> String {
    let mut parts = vec![format!(
        "{BOLD}{plan}{RESET} {CYAN}{}/{}{RESET}",
        money(monthly_remaining),
        money(monthly_cap)
    )];
    if let Some((u, c)) = h5 {
        let pct = if c > 0.0 { u / c * 100.0 } else { 0.0 };
        parts.push(format!("{DIM}5h{RESET} {}", bar(pct, 10)));
    }
    if let Some((u, c)) = weekly {
        let pct = if c > 0.0 { u / c * 100.0 } else { 0.0 };
        parts.push(format!("{DIM}wk{RESET} {}", bar(pct, 10)));
    }
    parts.join(" \u{b7} ")
}
