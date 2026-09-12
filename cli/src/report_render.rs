use crate::render::{color_for, compact, money, BOLD, CYAN, DIM, RESET};

/// Output format for the report tables. JSON and CSV are machine-readable
/// (never colored); `Table` honors the caller's color decision.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fmt {
    Table,
    Json,
    Csv,
}

/// RFC 4180 field: quote when it holds a comma, quote, or newline.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// All token columns incl. cache read + write — the displayed "Tokens" total
/// must not silently drop cache-write spend.
fn tokens(t: &crate::reports::Totals) -> u64 {
    t.usage.input_tokens
        + t.usage.output_tokens
        + t.usage.cache_read_tokens
        + t.usage.cache_write_tokens
}

pub fn bar(pct: f64, width: usize, ascii: bool, colors: bool) -> String {
    let filled = ((pct / 100.0).clamp(0.0, 1.0) * width as f64).round() as usize;
    let (full, empty) = if ascii { ("#", "-") } else { ("━", "╱") };
    if !colors {
        return format!(
            "{pct:>5.1}% {}{}",
            full.repeat(filled),
            empty.repeat(width - filled)
        );
    }
    let color = color_for(pct);
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
    /// Pre-rendered "on pace to hit cap in …" text, if a pace warning applies.
    pub five_hour_eta: Option<String>,
    pub weekly_eta: Option<String>,
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
///   {5h_eta}        5-hour pace ETA ("on pace to hit cap in 2h 3m") or empty
///   {wk_bar}        weekly bar
///   {wk_pct} {wk_used} {wk_cap}
///   {wk_eta}        weekly pace ETA or empty
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
    out
}

fn placeholder(key: &str, d: &StatusData) -> String {
    let bar = |p: f64| bar(p, d.bar_width, d.ascii, d.colors);
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
        "5h_eta" => d.five_hour_eta.clone().unwrap_or_default(),
        "wk_bar" => bar(pct_of(d.weekly)),
        "wk_pct" => format!("{:.0}%", pct_of(d.weekly)),
        "wk_used" => money(d.weekly.map(|(u, _)| u).unwrap_or(0.0)),
        "wk_cap" => money(d.weekly.map(|(_, c)| c).unwrap_or(0.0)),
        "wk_eta" => d.weekly_eta.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

// ---- tables ----

fn totals_json(t: &crate::reports::Totals) -> serde_json::Value {
    serde_json::json!({
        "requests": t.requests,
        "tokensIn": t.usage.input_tokens,
        "tokensOut": t.usage.output_tokens,
        "cacheRead": t.usage.cache_read_tokens,
        "cacheWrite": t.usage.cache_write_tokens,
        "costUsd": t.usage.cost_usd,
    })
}

/// serde_json Map from sorted (key, Totals) pairs — keys are escaped.
fn totals_map<'a, I>(items: I) -> serde_json::Map<String, serde_json::Value>
where
    I: Iterator<Item = (&'a String, &'a crate::reports::Totals)>,
{
    items.map(|(k, t)| (k.clone(), totals_json(t))).collect()
}

fn table_header(colors: bool) -> String {
    let (b, r) = if colors { (BOLD, RESET) } else { ("", "") };
    format!(
        " {b}{:<12}{r} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
        "Day", "Reqs", "In", "Out", "Cache rd", "Cache wr", "Cost"
    )
}

fn table_row(day: &str, t: &crate::reports::Totals, dim: bool) -> String {
    let (d_open, d_close): (&str, &str) = if dim { (DIM, RESET) } else { ("", "") };
    format!(
        " {d_open}{:<12}{d_close} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
        day,
        compact(t.requests),
        compact(t.usage.input_tokens),
        compact(t.usage.output_tokens),
        compact(t.usage.cache_read_tokens),
        compact(t.usage.cache_write_tokens),
        money(t.usage.cost_usd),
    )
}

/// Daily usage table (local or account scope). JSON key and heading differ.
pub fn table(
    scope: &str,
    subtitle: &str,
    by_day: &crate::reports::ByDay,
    total: &crate::reports::Totals,
    days: Option<usize>,
    fmt: Fmt,
    colors: bool,
) -> String {
    match fmt {
        Fmt::Json => serde_json::json!({
            "scope": scope,
            "total": totals_json(total),
            "days": totals_map(by_day.iter()),
        })
        .to_string(),
        Fmt::Csv => {
            let mut o =
                String::from("day,requests,tokens_in,tokens_out,cache_read,cache_write,cost_usd\n");
            for (day, t) in by_day {
                o.push_str(&format!(
                    "{},{},{},{},{},{},{:.2}\n",
                    csv_field(day),
                    t.requests,
                    t.usage.input_tokens,
                    t.usage.output_tokens,
                    t.usage.cache_read_tokens,
                    t.usage.cache_write_tokens,
                    t.usage.cost_usd,
                ));
            }
            o.push_str(&format!(
                "total,{},{},{},{},{},{:.2}\n",
                total.requests,
                total.usage.input_tokens,
                total.usage.output_tokens,
                total.usage.cache_read_tokens,
                total.usage.cache_write_tokens,
                total.usage.cost_usd,
            ));
            o
        }
        Fmt::Table => {
            let (b, r) = if colors { (BOLD, RESET) } else { ("", "") };
            let (d, dr) = if colors { (DIM, RESET) } else { ("", "") };
            let mut o = format!(
                "{b}{}{r} {d}({subtitle}){dr}\n\n",
                if scope == "account" {
                    "Account usage"
                } else {
                    "Local usage"
                }
            );
            o.push_str(&table_header(colors));
            let day_count = by_day.len();
            let last_n = days.unwrap_or(usize::MAX);
            for (day, t) in by_day.iter().skip(day_count.saturating_sub(last_n)) {
                o.push_str(&table_row(day, t, false));
            }
            o.push_str(&table_row("total", total, colors));
            o
        }
    }
}

/// Hourly account usage table
pub fn hourly_table(
    rows: &[(String, crate::reports::Totals)],
    fmt: Fmt,
    source: &str,
    colors: bool,
) -> String {
    match fmt {
        Fmt::Json => serde_json::json!({
            "scope": "hourly",
            "source": source.replace(' ', "-"),
            "hours": totals_map(rows.iter().map(|(h, t)| (h, t))),
        })
        .to_string(),
        Fmt::Csv => {
            let mut o = String::from("hour,requests,tokens_in,tokens_out,cost_usd\n");
            for (h, t) in rows {
                o.push_str(&format!(
                    "{},{},{},{},{:.2}\n",
                    csv_field(h),
                    t.requests,
                    t.usage.input_tokens,
                    t.usage.output_tokens,
                    t.usage.cost_usd,
                ));
            }
            o
        }
        Fmt::Table => {
            let (b, r) = if colors { (BOLD, RESET) } else { ("", "") };
            let (d, dr) = if colors { (DIM, RESET) } else { ("", "") };
            let mut o = format!("{b}Usage by hour{r} {d}({source}){dr}\n\n");
            o.push_str(&format!(
                " {b}{:<14}{r} {:>6} {:>10} {:>10} {:>10}\n",
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
    }
}

pub fn project_table(by_project: &crate::reports::ByProject, fmt: Fmt, colors: bool) -> String {
    match fmt {
        Fmt::Json => serde_json::json!({ "projects": totals_map(by_project.iter()) }).to_string(),
        Fmt::Csv => {
            let mut o = String::from("project,requests,tokens,cost_usd\n");
            for (p, t) in by_project {
                o.push_str(&format!(
                    "{},{},{},{:.2}\n",
                    csv_field(p),
                    t.requests,
                    tokens(t),
                    t.usage.cost_usd,
                ));
            }
            o
        }
        Fmt::Table => {
            let (b, r) = if colors { (BOLD, RESET) } else { ("", "") };
            let mut o = format!("\n{b}By project{r}\n");
            o.push_str(&format!(
                " {b}{:<32}{r} {:>6} {:>10} {:>10}\n",
                "Project", "Reqs", "Tokens", "Cost"
            ));
            for (p, t) in by_project {
                o.push_str(&format!(
                    " {:<32} {:>6} {:>10} {:>10}\n",
                    p,
                    compact(t.requests),
                    compact(tokens(t)),
                    money(t.usage.cost_usd),
                ));
            }
            o
        }
    }
}

pub fn model_table(by_model: &crate::reports::ByModel, fmt: Fmt, colors: bool) -> String {
    match fmt {
        Fmt::Json => serde_json::json!({ "models": totals_map(by_model.iter()) }).to_string(),
        Fmt::Csv => {
            let mut o = String::from("model,requests,tokens,cost_usd\n");
            for (m, t) in by_model {
                o.push_str(&format!(
                    "{},{},{},{:.2}\n",
                    csv_field(m),
                    t.requests,
                    tokens(t),
                    t.usage.cost_usd,
                ));
            }
            o
        }
        Fmt::Table => {
            let (b, r) = if colors { (BOLD, RESET) } else { ("", "") };
            let (d, dr) = if colors { (DIM, RESET) } else { ("", "") };
            let mut o = format!("\n{b}By model{r}\n");
            o.push_str(&format!(
                " {b}{:<32}{r} {:>6} {:>10} {:>10}\n",
                "Model", "Reqs", "Tokens", "Cost"
            ));
            let total_cost: f64 = by_model.values().map(|t| t.usage.cost_usd).sum();
            for (m, t) in by_model {
                let share = if total_cost > 0.0 {
                    format!(" {d}{:>5.1}%{dr}", t.usage.cost_usd / total_cost * 100.0)
                } else {
                    String::new()
                };
                o.push_str(&format!(
                    " {:<32} {:>6} {:>10} {:>10}{share}\n",
                    m,
                    compact(t.requests),
                    compact(tokens(t)),
                    money(t.usage.cost_usd),
                ));
            }
            o
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::{ByProject, Totals, Usage};

    #[test]
    fn project_json_escapes_keys() {
        let mut by: ByProject = ByProject::new();
        let t = Totals {
            requests: 1,
            usage: Usage {
                cost_usd: 0.5,
                ..Default::default()
            },
        };
        by.insert("we\"ird\\name".into(), t);
        let out = project_table(&by, Fmt::Json, false);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["projects"]["we\"ird\\name"]["requests"], 1);
    }

    #[test]
    fn csv_quotes_and_includes_cache_write() {
        let mut by: ByProject = ByProject::new();
        by.insert(
            "a,b".into(),
            Totals {
                requests: 2,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_tokens: 30,
                    cache_write_tokens: 40,
                    cost_usd: 1.5,
                },
            },
        );
        let out = project_table(&by, Fmt::Csv, false);
        assert_eq!(
            out,
            "project,requests,tokens,cost_usd\n\"a,b\",2,100,1.50\n"
        );
    }

    #[test]
    fn table_rows_include_cache_write_column() {
        let by: crate::reports::ByDay = [(
            "2026-09-27".to_string(),
            Totals {
                requests: 1,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 2,
                    cache_read_tokens: 3,
                    cache_write_tokens: 4,
                    cost_usd: 0.1,
                },
            },
        )]
        .into_iter()
        .collect();
        let total = crate::reports::sum_days(&by);
        let out = table("local", "test", &by, &total, None, Fmt::Table, false);
        assert!(out.contains("Cache wr"));
        assert!(out.contains(" 4 "));
    }
}
