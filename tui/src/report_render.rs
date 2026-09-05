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

pub fn bar(pct: f64, width: usize, ascii: bool) -> String {
    let filled = ((pct / 100.0).clamp(0.0, 1.0) * width as f64).round() as usize;
    let color = if pct >= 90.0 {
        RED
    } else if pct >= 70.0 {
        YELLOW
    } else {
        GREEN
    };
    let (full, empty) = if ascii { ("#", "-") } else { ("━", "╱") };
    format!(
        "{color}{pct:>5.1}%{} {}{}{}",
        RESET,
        full.repeat(filled),
        DIM,
        empty.repeat(width - filled)
    )
}

fn pct_of(w: &Option<(f64, f64)>) -> f64 {
    match w {
        Some((u, c)) if *c > 0.0 => u / c * 100.0,
        _ => 0.0,
    }
}

/// What a statusline can show. Fields map 1:1 to template placeholders.
pub struct StatusData<'a> {
    pub plan: &'a str,
    pub monthly_remaining: f64,
    pub monthly_cap: f64,
    pub five_hour: &'a Option<(f64, f64)>,
    pub weekly: &'a Option<(f64, f64)>,
    pub bar_width: usize,
    pub colors: bool,
    pub ascii: bool,
}

/// Template placeholders (case-insensitive):
///   {plan}          plan name
///   {credits}       remaining monthly credits ($x.xx)
///   {cap}           monthly cap ($x.xx)
///   {credits_bar}   monthly usage bar (used%)
///   {5h_bar}        5-hour window bar
///   {5h_pct}        5-hour used %
///   {5h_used} {5h_cap}
///   {wk_bar}        weekly bar
///   {wk_pct} {wk_used} {wk_cap}
///   | or newline    segment separators
/// Unknown placeholders are dropped. Plain text passes through.
pub fn render_statusline(tpl: &str, d: &StatusData) -> String {
    let mut out = String::new();
    let mut rest = tpl;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let key = after[..end].to_lowercase();
                out.push_str(&placeholder(&key, d));
                rest = &after[end + 1..];
            }
            None => {
                // no closing brace: emit rest verbatim
                out.push('{');
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    if d.colors {
        out
    } else {
        strip_ansi(&out)
    }
}

fn placeholder(key: &str, d: &StatusData) -> String {
    let bar = |p: f64| bar(p, d.bar_width, d.ascii);
    match key {
        "plan" => d.plan.to_string(),
        "credits" => {
            if d.colors {
                format!("{CYAN}{}{RESET}", money(d.monthly_remaining))
            } else {
                money(d.monthly_remaining)
            }
        }
        "cap" => money(d.monthly_cap),
        "credits_bar" => {
            let pct = if d.monthly_cap > 0.0 {
                (d.monthly_cap - d.monthly_remaining) / d.monthly_cap * 100.0
            } else {
                0.0
            };
            bar(pct)
        }
        "5h_bar" => bar(pct_of(d.five_hour)),
        "5h_pct" => format!("{:.0}%", pct_of(d.five_hour)),
        "5h_used" => money(d.five_hour.map(|(u, _)| u).unwrap_or(0.0)),
        "5h_cap" => money(d.five_hour.map(|(_, c)| c).unwrap_or(0.0)),
        "wk_bar" => bar(pct_of(d.weekly)),
        "wk_pct" => format!("{:.0}%", pct_of(d.weekly)),
        "wk_used" => money(d.weekly.map(|(u, _)| u).unwrap_or(0.0)),
        "wk_cap" => money(d.weekly.map(|(_, c)| c).unwrap_or(0.0)),
        _ => String::new(),
    }
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // skip until letter terminating the sequence
            for c2 in chars.by_ref() {
                if c2.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---- tables ----

pub fn total_json(t: &crate::reports::Totals) -> String {
    format!(
        "{{\"requests\":{},\"tokensIn\":{},\"tokensOut\":{},\"cacheRead\":{},\"cacheWrite\":{},\"costUsd\":{:.4}}}",
        t.requests,
        t.usage.input_tokens,
        t.usage.output_tokens,
        t.usage.cache_read_tokens,
        t.usage.cache_write_tokens,
        t.usage.cost_usd
    )
}

fn table_header() -> String {
    format!(
        " {BOLD}{:<12}{RESET} {:>6} {:>10} {:>10} {:>10} {:>10}\n",
        "Day", "Reqs", "In", "Out", "Cache rd", "Cost"
    )
}

fn table_row(day: &str, t: &crate::reports::Totals, dim: bool) -> String {
    let (d_open, d_close): (&str, &str) = if dim { (DIM, RESET) } else { ("", "") };
    format!(
        " {d_open}{:<12}{d_close} {:>6} {:>10} {:>10} {:>10} {:>10}\n",
        day,
        compact(t.requests),
        compact(t.usage.input_tokens),
        compact(t.usage.output_tokens),
        compact(t.usage.cache_read_tokens),
        money(t.usage.cost_usd),
    )
}

/// Local (CLI-only, offline) daily table
pub fn table(by_day: &crate::reports::ByDay, total: &crate::reports::Totals, days: Option<usize>, json: bool) -> String {
    if json {
        let ds: Vec<String> = by_day
            .iter()
            .map(|(d, t)| format!("\"{}\":{}", d, total_json(t)))
            .collect();
        return format!(
            "{{\"scope\":\"local\",\"total\":{},\"days\":{{{}}}}}",
            total_json(total),
            ds.join(",")
        );
    }
    let mut o = format!(
        "{BOLD}Local usage{RESET} {DIM}(offline, ~/.commandcode/projects){RESET}\n\n"
    );
    o.push_str(&table_header());
    let day_count = by_day.len();
    let last_n = days.unwrap_or(usize::MAX);
    for (day, t) in by_day.iter().skip(day_count.saturating_sub(last_n)) {
        o.push_str(&table_row(day, t, false));
    }
    o.push_str(&table_row("total", total, true));
    o
}

/// Account-wide daily table (all harnesses, from usage API)
pub fn account_table(by_day: &crate::reports::ByDay, total: &crate::reports::Totals, json: bool) -> String {
    if json {
        let ds: Vec<String> = by_day
            .iter()
            .map(|(d, t)| format!("\"{}\":{}", d, total_json(t)))
            .collect();
        return format!(
            "{{\"scope\":\"account\",\"total\":{},\"days\":{{{}}}}}",
            total_json(total),
            ds.join(",")
        );
    }
    let mut o = format!(
        "{BOLD}Account usage{RESET} {DIM}(all harnesses){RESET}\n\n"
    );
    o.push_str(&table_header());
    for (day, t) in by_day {
        o.push_str(&table_row(day, t, false));
    }
    o.push_str(&table_row("total", total, true));
    o
}

/// Hourly account usage table
pub fn hourly_table(rows: &[(String, crate::reports::Totals)], json: bool, source: &str) -> String {
    if json {
        let items: Vec<String> = rows
            .iter()
            .map(|(h, t)| format!("\"{}\":{}", h, total_json(t)))
            .collect();
        return format!(
            "{{\"scope\":\"hourly\",\"source\":\"{}\",\"hours\":{{{}}}}}",
            source.replace(" ", "-"),
            items.join(",")
        );
    }
    let mut o = format!(
        "{BOLD}Usage by hour{RESET} {DIM}({source}){RESET}\n\n"
    );
    o.push_str(&format!(
        " {BOLD}{:<14}{RESET} {:>6} {:>10} {:>10} {:>10}\n",
        "Hour", "Reqs", "In", "Out", "Cost"
    ));
    for (h, t) in rows {
        o.push_str(&format!(
            " {:<14} {:>6} {:>10} {:>10} {:>10}\n",
            h,
            compact(t.requests),
            compact(t.usage.input_tokens),
            compact(t.usage.output_tokens),
            money(t.usage.cost_usd),
        ));
    }
    o
}

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
