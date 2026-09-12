use zed_extension_api::http_client::{HttpMethod, HttpRequest, RedirectPolicy};
use zed_extension_api::{
    process::Command, register_extension, Extension, Range, SlashCommand, SlashCommandOutput,
    SlashCommandOutputSection, Worktree,
};

use cmduse_core::{
    compact, elapsed_pct, gate_age_days, money, pct, plan_monthly_cap, plan_name,
    plan_rule_matched, rel_time, CreditsResp, SubscriptionsResp, UsageSummary, Window,
    GATE_CLI_VERSION, PLANS,
};

const API_BASE: &str = "https://api.commandcode.ai";
const BAR_WIDTH: usize = 12;

struct CommandCodeUsage;

fn get_api_key_and_now() -> Result<(String, Option<u64>), String> {
    let out = Command::new("sh")
        .arg("-c")
        .arg("cat \"$HOME/.commandcode/auth.json\" && printf \"\\n__CMDNOW__\" && date +%s")
        .output()
        .map_err(|e| format!("failed to spawn sh: {e}"))?;
    if out.status != Some(0) {
        return Err("could not read ~/.commandcode/auth.json — is Command Code CLI installed and logged in? Run `cmd login` in your terminal.".into());
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut now = None;
    let json_part = match text.split_once("__CMDNOW__") {
        Some((json, tail)) => {
            if let Ok(secs) = tail.trim().parse::<u64>() {
                now = Some(secs);
            }
            json
        }
        None => text.as_str(),
    };
    let v: serde_json::Value = serde_json::from_str(json_part.trim())
        .map_err(|e| format!("auth.json parse error: {e}"))?;
    let key = v["apiKey"]
        .as_str()
        .ok_or("auth.json has no apiKey — run `cmd login`")?
        .to_string();
    Ok((key, now))
}

/// GET a JSON endpoint, returning the raw body. `zed_extension_api`'s WIT
/// HttpResponse has no status field, so non-200 surfaces as a JSON parse error
/// at the call site.
fn http_get_json(path: &str, key: &str) -> Result<Vec<u8>, String> {
    let req = HttpRequest::builder()
        .method(HttpMethod::Get)
        .url(format!("{API_BASE}{path}"))
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json")
        .redirect_policy(RedirectPolicy::FollowAll)
        .build()?;
    let resp = req.fetch().map_err(|e| format!("{path}: {e}"))?;
    Ok(resp.body)
}

fn bar(used: f64, cap: f64) -> String {
    let pct = if cap > 0.0 {
        (used / cap).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = (pct * BAR_WIDTH as f64).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(BAR_WIDTH - filled))
}

fn window_line(label: &str, w: &Window, now: Option<u64>, dur_secs: Option<u64>) -> String {
    let status = if w.exceeded {
        " · **LIMIT EXCEEDED**"
    } else {
        ""
    };
    let thru = dur_secs
        .and_then(|d| elapsed_pct(w.reset_at, d, now))
        .map(|p| format!(" · window {p}% elapsed"))
        .unwrap_or_default();
    // burn-rate: pace vs cap, only warn if cap-hit lands before reset.
    // ponytail: flat-rate assumption; bursty sessions shift the ETA.
    // upgrade: real fix = server-side rate history; revisit if ETA misfires in practice.
    let pace = dur_secs
        .zip(now)
        .and_then(|(d, now_s)| {
            cmduse_core::pace_eta(w.reset_at, d, w.used, w.cap, now_s)
                .map(|secs| cmduse_core::duration(secs as u64))
        })
        .map(|eta| format!(" · **on pace to hit cap in {eta}**"))
        .unwrap_or_default();
    format!(
        "**{label}** `{}` {} of {} ({}) · resets in {}{thru}{pace}{}\n",
        bar(w.used, w.cap),
        money(w.used),
        money(w.cap),
        pct(w.used, w.cap),
        rel_time(w.reset_at, now),
        status
    )
}

/// Rendered line index where the next pushed element begins. Elements may
/// carry trailing '\n' and `join("\n")` inserts a separator, so section
/// ranges must count rendered lines, not Vec elements.
fn next_line(lines: &[String], line_count: u32) -> u32 {
    line_count + if lines.is_empty() { 0 } else { 1 }
}

/// Push one output element, advancing `line_count` past the join separator
/// and any newlines the element itself contains.
fn push_line(lines: &mut Vec<String>, line_count: &mut u32, s: impl Into<String>) {
    if !lines.is_empty() {
        *line_count += 1; // join("\n") separator
    }
    let s = s.into();
    *line_count += s.matches('\n').count() as u32;
    lines.push(s);
}

fn plans_table(current: &str) -> String {
    // Mark by exact plan_name match (not substring): "individual-goat"
    // contains "go", so substring matching double-marks the Go row. Empty id
    // = unknown plan: mark nothing (would otherwise fall through to "Free").
    let mine = if current.is_empty() {
        ""
    } else {
        plan_name(current)
    };
    let mut out =
        String::from("| Plan | Price | Credits/mo | 5-hour | Weekly |\n|---|---|---|---|---|\n");
    for &(name, price, monthly, h5, wk) in PLANS {
        let mark = if name == mine { "**" } else { "" };
        out.push_str(&format!(
            "| {mark}{name}{mark} | {price}/mo | {monthly} | {h5} | {wk} |\n"
        ));
    }
    out.push_str("\nWindows throttle only included monthly credits; on-demand (`/extra`) credits are never throttled.\n");
    out
}

impl Extension for CommandCodeUsage {
    fn new() -> Self {
        Self
    }

    fn run_slash_command(
        &self,
        _command: SlashCommand,
        args: Vec<String>,
        _worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        let arg = args
            .first()
            .map(|a| a.trim().to_string())
            .unwrap_or_default();

        if arg == "plans" {
            let text = format!("## Command Code Plans\n\n{}", plans_table(""));
            return Ok(SlashCommandOutput {
                sections: vec![SlashCommandOutputSection {
                    range: Range {
                        start: 0,
                        end: text.lines().count() as u32,
                    },
                    label: "Plans".into(),
                }],
                text,
            });
        }

        let (key, now) = get_api_key_and_now()?;

        let mut lines: Vec<String> = Vec::new();
        let mut section_starts: Vec<(u32, &str)> = Vec::new();
        let mut line_count: u32 = 0;

        // 1. plan + credits
        section_starts.push((next_line(&lines, line_count), "Plan & Credits"));
        // zed's HttpResponse carries no status, so an error body would otherwise
        // deserialize to no `data` and render as a fake "Free" plan.
        let raw = http_get_json("/alpha/billing/subscriptions", &key)?;
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&raw) {
            if v.get("error").is_some() && v.get("data").is_none() {
                let msg = v
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("request failed");
                return Err(format!("subscriptions: {msg}"));
            }
        }
        let sub: SubscriptionsResp =
            serde_json::from_slice(&raw).map_err(|e| format!("subscriptions parse: {e}"))?;
        let sub_data = sub.data_or_free();
        push_line(
            &mut lines,
            &mut line_count,
            format!(
                "## Command Code — {} ({})\n",
                plan_name(&sub_data.plan_id),
                sub_data.status
            ),
        );
        if !sub_data.plan_id.is_empty()
            && sub_data.plan_id != "free"
            && !plan_rule_matched(&sub_data.plan_id)
        {
            push_line(
                &mut lines,
                &mut line_count,
                format!(
                    "> warning: unknown plan id `{}` — showing Free; update core/plans.json\n",
                    sub_data.plan_id
                ),
            );
        }
        if let Some(days) = now.and_then(gate_age_days) {
            if days > 30 {
                push_line(
                    &mut lines,
                    &mut line_count,
                    format!(
                        "> warning: gating snapshot is {days}d old (CLI {GATE_CLI_VERSION}) — run `bun run extract` from the repo root\n"
                    ),
                );
            }
        }
        if let Some(end) = &sub_data.current_period_end {
            push_line(
                &mut lines,
                &mut line_count,
                format!("Billing period ends `{}`\n", &end[..10.min(end.len())]),
            );
        }

        let credits: CreditsResp =
            serde_json::from_slice(&http_get_json("/alpha/billing/credits", &key)?)
                .map_err(|e| format!("credits parse: {e}"))?;
        let cap = plan_monthly_cap(&sub_data.plan_id);
        match cap {
            Some(c) => push_line(
                &mut lines,
                &mut line_count,
                format!(
                    "**Credits:** {} / {} monthly · {} purchased · {} free\n",
                    money(credits.credits.monthly_credits),
                    money(c),
                    money(credits.credits.purchased_credits),
                    money(credits.credits.free_credits),
                ),
            ),
            None => push_line(
                &mut lines,
                &mut line_count,
                format!(
                    "**Credits remaining:** {} monthly · {} purchased · {} free\n",
                    money(credits.credits.monthly_credits),
                    money(credits.credits.purchased_credits),
                    money(credits.credits.free_credits),
                ),
            ),
        }

        // 2. usage windows
        section_starts.push((next_line(&lines, line_count), "Usage Windows"));
        push_line(&mut lines, &mut line_count, "### Usage windows");
        if let Some(c) = cap {
            let (w, dur) = cmduse_core::monthly_window(
                c,
                credits.credits.monthly_credits,
                sub_data.current_period_start.as_deref(),
                sub_data.current_period_end.as_deref(),
            );
            push_line(
                &mut lines,
                &mut line_count,
                window_line("Monthly", &w, now, dur),
            );
        }
        if let Some(w) = &credits.window_limits.five_hour {
            push_line(
                &mut lines,
                &mut line_count,
                window_line("5-hour", w, now, Some(cmduse_core::FIVE_HOUR_SECS)),
            );
        }
        if let Some(w) = &credits.window_limits.weekly {
            push_line(
                &mut lines,
                &mut line_count,
                window_line("Weekly", w, now, Some(cmduse_core::WEEKLY_SECS)),
            );
        }
        if credits.window_limits.five_hour.is_none() && credits.window_limits.weekly.is_none() {
            push_line(
                &mut lines,
                &mut line_count,
                "No rolling windows on this plan (pay-as-you-go credits only).\n",
            );
        }

        // 3. usage summary (optional: skip the section if unavailable)
        section_starts.push((next_line(&lines, line_count), "Billing Period Usage"));
        push_line(&mut lines, &mut line_count, "### This billing period");
        let summary = http_get_json("/alpha/usage/summary", &key)
            .ok()
            .and_then(|b| serde_json::from_slice::<UsageSummary>(&b).ok());
        match &summary {
            Some(sm) => {
                push_line(&mut lines, &mut line_count, "| Metric | Value |");
                push_line(&mut lines, &mut line_count, "|---|---|");
                push_line(
                    &mut lines,
                    &mut line_count,
                    format!(
                        "| Requests | {} |",
                        sm.total_count.map_or("—".into(), |v| v.to_string())
                    ),
                );
                push_line(
                    &mut lines,
                    &mut line_count,
                    format!("| Cost | {} |", sm.total_cost.map_or("—".into(), money)),
                );
                push_line(
                    &mut lines,
                    &mut line_count,
                    format!(
                        "| Tokens in / out | {} / {} |",
                        sm.total_tokens_in.map_or("—".into(), compact),
                        sm.total_tokens_out.map_or("—".into(), compact),
                    ),
                );
                push_line(
                    &mut lines,
                    &mut line_count,
                    format!(
                        "| Success rate | {} |",
                        sm.success_rate.map_or("—".into(), |r| format!("{r:.0}%"))
                    ),
                );
                push_line(&mut lines, &mut line_count, String::new());
            }
            None => push_line(
                &mut lines,
                &mut line_count,
                "_Usage summary unavailable._\n",
            ),
        }

        // 4. plans reference
        section_starts.push((next_line(&lines, line_count), "Plan Reference"));
        push_line(&mut lines, &mut line_count, "### All plans");
        push_line(&mut lines, &mut line_count, plans_table(&sub_data.plan_id));

        let text = lines.join("\n");
        let total = text.lines().count() as u32;
        let sections = section_starts
            .into_iter()
            .map(|(start, label)| SlashCommandOutputSection {
                range: Range { start, end: total },
                label: label.into(),
            })
            .collect();

        Ok(SlashCommandOutput { text, sections })
    }
}

register_extension!(CommandCodeUsage);

#[cfg(test)]
mod tests {
    use super::*;

    // Plan names/caps, money, compact, rel_time, and ISO parsing are covered by
    // the shared conformance vectors in core; only presentation is asserted here.

    #[test]
    fn elapsed_window() {
        let now = 1_000_000u64;
        // 5h window ending now+2.5h → 50% elapsed
        let reset = (now as f64 + 2.5 * 3600.0) * 1000.0;
        assert_eq!(elapsed_pct(Some(reset), 5 * 3600, Some(now)), Some(50));
        assert_eq!(elapsed_pct(None, 3600, Some(now)), None);
    }

    #[test]
    fn pace_warns_only_after_10pct_elapsed() {
        let now = 1_000_000u64;
        let d = 5 * 3600;
        let mk = |start: u64| Window {
            used: 5.0,
            cap: 10.0,
            exceeded: false,
            reset_at: Some((start + d) as f64 * 1000.0),
        };
        let early = window_line("5-hour", &mk(now - d / 20), Some(now), Some(d));
        assert!(
            !early.contains("on pace"),
            "must not warn at 5% elapsed: {early}"
        );
        let at10 = window_line("5-hour", &mk(now - d / 10), Some(now), Some(d));
        assert!(
            at10.contains("on pace to hit cap in 30m"),
            "ETA must render the duration, not an absolute reset: {at10}"
        );
    }

    #[test]
    fn bars() {
        assert_eq!(bar(0.0, 10.0), "░".repeat(BAR_WIDTH));
        assert_eq!(bar(10.0, 10.0), "█".repeat(BAR_WIDTH));
        assert_eq!(bar(5.0, 10.0).chars().filter(|c| *c == '█').count(), 6);
    }

    #[test]
    fn section_ranges_count_rendered_lines() {
        // Elements carry trailing '\n' + join separators, so section starts
        // must count rendered lines, not Vec elements.
        let mut lines = Vec::new();
        let mut n = 0u32;
        let s1 = next_line(&lines, n);
        push_line(&mut lines, &mut n, "a\n");
        let s2 = next_line(&lines, n);
        push_line(&mut lines, &mut n, "b\n");
        push_line(&mut lines, &mut n, "c");
        let text = lines.join("\n"); // "a\n\nb\n\nc" → 5 lines
        assert_eq!(s1, 0);
        assert_eq!(s2, 2);
        assert_eq!(text.lines().count() as u32, 5);
    }
}
