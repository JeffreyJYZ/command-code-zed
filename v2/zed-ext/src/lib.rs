use serde::Deserialize;
use zed_extension_api::http_client::{HttpMethod, HttpRequest, RedirectPolicy};
use zed_extension_api::{
    process::Command, register_extension, Extension, Range, SlashCommand, SlashCommandOutput,
    SlashCommandOutputSection, Worktree,
};

use cmduse_core::{compact, elapsed_pct, money, parse_iso_utc, pct, plan_monthly_cap, plan_name, rel_time};

const API_BASE: &str = "https://api.commandcode.ai";
const BAR_WIDTH: usize = 12;

struct CommandCodeUsage;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credits {
    monthly_credits: f64,
    #[serde(default)]
    purchased_credits: f64,
    #[serde(default)]
    free_credits: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    used: f64,
    cap: f64,
    #[serde(default)]
    exceeded: bool,
    #[serde(default)]
    reset_at: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct WindowLimits {
    #[serde(default)]
    five_hour: Option<Window>,
    #[serde(default)]
    weekly: Option<Window>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditsResp {
    credits: Credits,
    #[serde(default)]
    window_limits: WindowLimits,
}

#[derive(Deserialize)]
struct SubscriptionsResp {
    #[serde(default)]
    data: Option<SubData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubData {
    #[serde(default)]
    status: String,
    #[serde(default)]
    plan_id: String,
    #[serde(default)]
    current_period_start: Option<String>,
    #[serde(default)]
    current_period_end: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageSummary {
    #[serde(default)]
    total_count: u64,
    #[serde(default)]
    total_cost: f64,
    #[serde(default)]
    success_rate: f64,
    #[serde(default)]
    total_tokens_in: u64,
    #[serde(default)]
    total_tokens_out: u64,
}

fn get_api_key_and_now() -> Result<(String, Option<u64>), String> {
    let out = Command::new("sh")
        .arg("-c")
        .arg("cat \"$HOME/.commandcode/auth.json\"; printf \"\\n__CMDNOW__\"; date +%s")
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
    let v: serde_json::Value =
        serde_json::from_str(json_part.trim()).map_err(|e| format!("auth.json parse error: {e}"))?;
    let key = v["apiKey"]
        .as_str()
        .ok_or("auth.json has no apiKey — run `cmd login`")?
        .to_string();
    Ok((key, now))
}

fn http_get_json(path: &str, key: &str) -> Result<Vec<u8>, String> {
    let req = HttpRequest::builder()
        .method(HttpMethod::Get)
        .url(format!("{API_BASE}{path}"))
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json")
        .redirect_policy(RedirectPolicy::FollowAll)
        .build()?;
    let resp = req.fetch().map_err(|e| format!("{path}: {e}"))?;
    // ponytail: WIT HttpResponse has no status field; non-200 surfaces as JSON parse error.
    // upgrade: re-check on every zed_extension_api bump (0.7.0 today) for a status field.
    serde_json::from_slice::<serde_json::Value>(&resp.body)
        .map_err(|e| format!("{path}: HTTP error or bad JSON: {e}"))?;
    Ok(resp.body)
}

fn bar(used: f64, cap: f64) -> String {
    let pct = if cap > 0.0 { (used / cap).clamp(0.0, 1.0) } else { 0.0 };
    let filled = (pct * BAR_WIDTH as f64).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(BAR_WIDTH - filled))
}

/// window duration secs from label (5h window, 7d weekly, monthly = period span)
fn window_dur_secs(label: &str) -> u64 {
    if label == "5-hour" { 5 * 3600 } else { 7 * 86400 }
}

fn window_line(label: &str, w: &Window, now: Option<u64>, dur_secs: Option<u64>) -> String {
    let status = if w.exceeded { " · **LIMIT EXCEEDED**" } else { "" };
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
                .map(|secs| rel_time(Some(secs * 1000.0), Some(now_s)))
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

fn plans_table(current: &str) -> String {
    let plans = [
        ("Go", "$1", "$10", "$3", "$6"),
        ("GOAT", "$10", "$70", "$14", "$35"),
        ("Pro", "$20", "$80", "$16", "$40"),
        ("Provider", "$15", "PAYG", "—", "—"),
        ("Max 10x", "$100", "$150", "$45", "$90"),
        ("Max 20x", "$200", "$300", "$90", "$180"),
        ("Team Pro", "$40", "$40", "$12", "$24"),
    ];
    // Mark by exact plan_name match (not substring): "individual-goat"
    // contains "go", so substring matching double-marks the Go row.
    let mine = plan_name(current);
    let mut out = String::from("| Plan | Price | Credits/mo | 5-hour | Weekly |\n|---|---|---|---|---|\n");
    for (name, price, monthly, h5, wk) in plans {
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
        let arg = args.first().map(|a| a.trim().to_string()).unwrap_or_default();

        if arg == "plans" {
            let text = format!("## Command Code Plans\n\n{}", plans_table(""));
            return Ok(SlashCommandOutput {
                sections: vec![SlashCommandOutputSection {
                    range: Range { start: 0, end: text.lines().count() as u32 },
                    label: "Plans".into(),
                }],
                text,
            });
        }

        let (key, now) = get_api_key_and_now()?;

        let mut lines: Vec<String> = Vec::new();
        let mut section_starts: Vec<(usize, &str)> = Vec::new();

        // 1. plan + credits
        section_starts.push((lines.len(), "Plan & Credits"));
        let sub: SubscriptionsResp =
            serde_json::from_slice(&http_get_json("/alpha/billing/subscriptions", &key)?)
                .map_err(|e| format!("subscriptions parse: {e}"))?;
        let sub_data = sub.data.unwrap_or(SubData {
            status: "none".into(),
            plan_id: "free".into(),
            current_period_start: None,
            current_period_end: None,
        });
        lines.push(format!(
            "## Command Code — {} ({})\n",
            plan_name(&sub_data.plan_id),
            sub_data.status
        ));
        if let Some(end) = &sub_data.current_period_end {
            lines.push(format!("Billing period ends `{}`\n", &end[..10.min(end.len())]));
        }

        let credits: CreditsResp =
            serde_json::from_slice(&http_get_json("/alpha/billing/credits", &key)?)
                .map_err(|e| format!("credits parse: {e}"))?;
        let cap = plan_monthly_cap(&sub_data.plan_id);
        match cap {
            Some(c) => lines.push(format!(
                "**Credits:** {} / {} monthly · {} purchased · {} free\n",
                money(credits.credits.monthly_credits),
                money(c),
                money(credits.credits.purchased_credits),
                money(credits.credits.free_credits),
            )),
            None => lines.push(format!(
                "**Credits remaining:** {} monthly · {} purchased · {} free\n",
                money(credits.credits.monthly_credits),
                money(credits.credits.purchased_credits),
                money(credits.credits.free_credits),
            )),
        }

        // 2. usage windows
        section_starts.push((lines.len(), "Usage Windows"));
        lines.push("### Usage windows".into());
        if let Some(c) = cap {
            let used = (c - credits.credits.monthly_credits).clamp(0.0, c);
            let reset_at = sub_data.current_period_end.as_ref().and_then(|e| parse_iso_utc(e));
            let dur = match (&sub_data.current_period_start, &sub_data.current_period_end) {
                (Some(st), Some(en)) => parse_iso_utc(st)
                    .zip(parse_iso_utc(en))
                    .map(|(a, b)| ((b - a) as u64 / 1000).max(1)),
                _ => None,
            };
            lines.push(window_line("Monthly", &Window { used, cap: c, exceeded: false, reset_at }, now, dur));
        }
        if let Some(w) = &credits.window_limits.five_hour {
            lines.push(window_line("5-hour", w, now, Some(window_dur_secs("5-hour"))));
        }
        if let Some(w) = &credits.window_limits.weekly {
            lines.push(window_line("Weekly", w, now, Some(window_dur_secs("Weekly"))));
        }
        if credits.window_limits.five_hour.is_none() && credits.window_limits.weekly.is_none() {
            lines.push("No rolling windows on this plan (pay-as-you-go credits only).\n".into());
        }

        // 3. usage summary
        section_starts.push((lines.len(), "Billing Period Usage"));
        lines.push("### This billing period".into());
        let summary: UsageSummary =
            serde_json::from_slice(&http_get_json("/alpha/usage/summary", &key)?)
                .map_err(|e| format!("summary parse: {e}"))?;
        lines.push("| Metric | Value |".into());
        lines.push("|---|---|".into());
        lines.push(format!("| Requests | {} |", summary.total_count));
        lines.push(format!("| Cost | {} |", money(summary.total_cost)));
        lines.push(format!(
            "| Tokens in / out | {} / {} |",
            compact(summary.total_tokens_in),
            compact(summary.total_tokens_out)
        ));
        lines.push(format!("| Success rate | {:.0}% |", summary.success_rate));
        lines.push(String::new());

        // 4. plans reference
        section_starts.push((lines.len(), "Plan Reference"));
        lines.push("### All plans".into());
        lines.push(plans_table(&sub_data.plan_id));

        let total = lines.len() as u32;
        let sections = section_starts
            .into_iter()
            .map(|(start, label)| SlashCommandOutputSection {
                range: Range { start: start as u32, end: total },
                label: label.into(),
            })
            .collect();

        Ok(SlashCommandOutput { text: lines.join("\n"), sections })
    }
}

register_extension!(CommandCodeUsage);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_names() {
        assert_eq!(plan_name("individual-goat"), "GOAT");
        assert_eq!(plan_name("individual-max-20"), "Max 20x");
        assert_eq!(plan_name("individual-go"), "Go");
        assert_eq!(plan_name("bogus"), "Free");
    }

    #[test]
    fn monthly_caps() {
        assert_eq!(plan_monthly_cap("individual-goat"), Some(70.0));
        assert_eq!(plan_monthly_cap("individual-go"), Some(10.0));
        assert_eq!(plan_monthly_cap("individual-pro"), Some(80.0));
        assert_eq!(plan_monthly_cap("individual-max-20"), Some(300.0));
        assert_eq!(plan_monthly_cap("teams-pro"), Some(40.0));
        assert_eq!(plan_monthly_cap("individual-provider"), None);
    }

    #[test]
    fn iso_and_elapsed() {
        assert_eq!(parse_iso_utc("2026-09-27T12:23:00.000Z").unwrap() as u64 / 1000, 1_790_511_780);
        assert_eq!(parse_iso_utc("nope"), None);
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
        assert!(!early.contains("on pace"), "must not warn at 5% elapsed: {early}");
        let at10 = window_line("5-hour", &mk(now - d / 10), Some(now), Some(d));
        assert!(at10.contains("on pace"), "should warn at 10% elapsed: {at10}");
    }

    #[test]
    fn bars_and_times() {
        assert_eq!(bar(0.0, 10.0), "░".repeat(BAR_WIDTH));
        assert_eq!(bar(10.0, 10.0), "█".repeat(BAR_WIDTH));
        assert_eq!(bar(5.0, 10.0).chars().filter(|c| *c == '█').count(), 6);
        assert_eq!(rel_time(Some(120_000.0), Some(60)), "1m");
        assert_eq!(rel_time(None, Some(0)), "unknown");
        assert_eq!(compact(28_129_791), "28.1M");
    }
}
