// I/O half of the reports: session-JSONL walk, the account API pool, and
// nothing else. Bucketing, cumulative-difference math, and the tz helpers live
// in `cmduse_core::reports` (single source, pure, unit-tested there).
use crate::api;
use cmduse_core::dates::{hour_label, now_secs, parse_iso_utc};
use cmduse_core::reports::{
    bucket_records, daily_from_cumulative, hour_bounds, hourly_from_cumulative, iso_day_start,
    local_hour_start, recent_days, today_in_tz, UsageRecord,
};
use serde::Deserialize;

// Re-exported so existing `crate::reports::…` paths (main, report_render) keep
// working unchanged.
pub use cmduse_core::reports::{sum_days, ByDay, ByModel, ByProject, LocalData, Totals, Usage};

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    role: Option<String>,
}

/// (project dir name, file contents) for every session JSONL under
/// `~/.commandcode/projects`, skipping checkpoints/metadata. Shared by the
/// daily and hourly local scans so the walk/filter lives in one place.
fn session_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(crate::paths::home().join(".commandcode/projects")) else {
        return out;
    };
    for proj in entries.flatten() {
        let proj_name = proj.file_name().to_string_lossy().to_string();
        let Ok(files) = std::fs::read_dir(proj.path()) else {
            continue;
        };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if !name.ends_with(".jsonl") || name.contains("checkpoints") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(f.path()) {
                out.push((proj_name.clone(), text));
            }
        }
    }
    out
}

/// Parse every session JSONL into owned usage records.
fn usage_records(files: &[(String, String)]) -> Vec<UsageRecord> {
    let mut out = Vec::new();
    for (proj_name, text) in files {
        let mut is_session_file = false;
        for line in text.lines() {
            let Ok(l) = serde_json::from_str::<Line>(line) else {
                continue;
            };
            match l.kind.as_str() {
                "session" => is_session_file = true,
                "message" => {
                    let Some(u) = l.usage else { continue };
                    // only assistant (model) messages carry usage; guard anyway
                    if l.message.as_ref().and_then(|m| m.role.as_deref()) == Some("user") {
                        continue;
                    }
                    out.push(UsageRecord {
                        project: proj_name.clone(),
                        timestamp: l.timestamp,
                        model: l.model,
                        usage: u,
                        is_session: is_session_file,
                    });
                }
                _ => {}
            }
        }
    }
    out
}

pub fn load_local(tz: i64) -> LocalData {
    let files = session_files();
    bucket_records(&usage_records(&files), tz)
}

// ---- Account-wide daily usage (API, all harnesses) ----
// The account API key can be used from any harness (CLI, other agents via
// Provider API). Only the server knows the full picture; local JSONL misses
// non-CLI usage. alpha/usage/summary?since=<ISO> returns cumulative totals
// from that instant to now. Per-day usage = cum(day start) - cum(next day
// start) — see core::reports::daily_from_cumulative.

/// Fetch cumulative summaries for many `since` boundaries with bounded
/// concurrency; result order matches input order.
/// ponytail: 8-way pool, no semaphore crate — spawn-and-join in chunks.
/// upgrade: tune POOL only if account reports ever hit latency limits.
fn fetch_pool(sinces: &[String], key: &str) -> Result<Vec<api::UsageSummary>, String> {
    const POOL: usize = 8;
    let mut out = Vec::new();
    let mut first_err: Option<String> = None;
    for chunk in sinces.chunks(POOL) {
        let handles: Vec<_> = chunk
            .iter()
            .map(|s| {
                let s = s.clone();
                let k = key.to_string();
                std::thread::spawn(move || api::summary_since(&s, &k))
            })
            .collect();
        // join every handle even after a failure: dropping a JoinHandle
        // detaches the thread, and watch mode would leak one per bad refresh.
        for h in handles {
            match h.join() {
                Ok(Ok(v)) => out.push(v),
                Ok(Err(e)) => {
                    first_err.get_or_insert(e);
                }
                Err(_) => {
                    first_err.get_or_insert_with(|| "usage thread panicked".to_string());
                }
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// Account-wide per-day usage for the last `days` days (today included),
/// fetched from the usage API (8-way concurrent). Includes usage from
/// every harness that used the account key.
pub fn load_account_daily(days: usize, key: &str, tz: i64) -> Result<ByDay, String> {
    let labels = recent_days(&today_in_tz(tz), days);
    let sinces: Vec<String> = labels.iter().map(|d| iso_day_start(d, tz)).collect();
    let cums = fetch_pool(&sinces, key)?;
    Ok(daily_from_cumulative(&labels, &cums))
}

/// Account-wide usage for the last `hours` hours, one row per hour bucket
/// (oldest first, current hour last). Includes all harnesses.
pub fn load_account_hourly(
    hours: usize,
    key: &str,
    tz: i64,
) -> Result<Vec<(String, Totals)>, String> {
    let (bounds_local, bounds_utc) = hour_bounds(now_secs(), hours.max(1), tz);
    let sinces: Vec<String> = bounds_utc
        .iter()
        .map(|&b| cmduse_core::dates::iso_instant(b))
        .collect();
    let cums = fetch_pool(&sinces, key)?;
    Ok(hourly_from_cumulative(&bounds_local, &cums))
}

/// Local hourly buckets from JSONL logs (offline, CLI sessions only).
/// Returns (label, totals) oldest-first for the last `hours` hours.
pub fn load_local_hourly(hours: usize, tz: i64) -> Vec<(String, Totals)> {
    let hours = hours.max(1);
    let files = session_files();
    let records = usage_records(&files);

    // local hour buckets, same math as the account path but from record stamps
    let now = now_secs();
    let (bounds_local, _) = hour_bounds(now, hours, tz);
    let oldest = bounds_local.first().copied().unwrap_or(now);
    let current = bounds_local.last().copied().unwrap_or(now);

    let mut by_hour: std::collections::BTreeMap<u64, Totals> = std::collections::BTreeMap::new();
    for r in &records {
        let Some(ms) = parse_iso_utc(&r.timestamp) else {
            continue;
        };
        let h = local_hour_start((ms / 1000.0) as u64, tz);
        if h >= oldest && h <= current {
            by_hour.entry(h).or_default().add(&r.usage);
        }
    }
    bounds_local
        .iter()
        .map(|&h| (hour_label(h), by_hour.get(&h).copied().unwrap_or_default()))
        .collect()
}
