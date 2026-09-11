use crate::api;
use cmduse_core::dates::{civil_from_days, day_shift, hour_label, iso_hour_start, now_secs, parse_iso_utc, today_utc, tz_offset_suffix};
use serde::Deserialize;
use std::collections::BTreeMap;
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

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default, rename = "costUsd")]
    pub cost_usd: f64,
}

#[derive(Default, Clone, Copy)]
pub struct Totals {
    pub requests: u64,
    pub usage: Usage,
}

impl Totals {
    fn add(&mut self, u: &Usage) {
        self.merge(&Totals { requests: 1, usage: *u });
    }

    fn merge(&mut self, o: &Totals) {
        self.requests += o.requests;
        self.usage.input_tokens += o.usage.input_tokens;
        self.usage.output_tokens += o.usage.output_tokens;
        self.usage.cache_read_tokens += o.usage.cache_read_tokens;
        self.usage.cache_write_tokens += o.usage.cache_write_tokens;
        self.usage.cost_usd += o.usage.cost_usd;
    }
}

/// day → totals (UTC date from message timestamp)
pub type ByDay = BTreeMap<String, Totals>;
/// model → totals
pub type ByModel = BTreeMap<String, Totals>;
/// project (dir name) → totals
pub type ByProject = BTreeMap<String, Totals>;

pub struct LocalData {
    pub by_day: ByDay,
    pub by_model: ByModel,
    pub by_project: ByProject,
    pub sessions: u64,
    pub total: Totals,
}

/// day key = UTC YYYY-MM-DD from ISO timestamp
fn day_of(ts: &str) -> Option<String> {
    ts.get(0..10).map(|s| s.to_string())
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

pub fn load_local() -> LocalData {
    let mut data = LocalData {
        by_day: ByDay::new(),
        by_model: ByModel::new(),
        by_project: ByProject::new(),
        sessions: 0,
        total: Totals::default(),
    };

    for (proj_name, text) in session_files() {
        let mut is_session_file = false;
        for line in text.lines() {
            let Ok(l) = serde_json::from_str::<Line>(line) else {
                continue;
            };
            match l.kind.as_str() {
                "session" => {
                    data.sessions += 1;
                    is_session_file = true;
                }
                "message" => {
                    let Some(u) = l.usage else { continue };
                    // only assistant (model) messages carry usage; guard anyway
                    if l.message.as_ref().and_then(|m| m.role.as_deref()) == Some("user") {
                        continue;
                    }
                    let bucket = |t: &mut Totals| t.add(&u);
                    if let Some(day) = day_of(&l.timestamp) {
                        bucket(data.by_day.entry(day).or_default());
                    }
                    if let Some(m) = &l.model {
                        bucket(data.by_model.entry(m.clone()).or_default());
                    }
                    if is_session_file {
                        bucket(data.by_project.entry(proj_name.clone()).or_default());
                    }
                    bucket(&mut data.total);
                }
                _ => {}
            }
        }
    }
    data
}

// ---- Account-wide daily usage (API, all harnesses) ----
// The account API key can be used from any harness (CLI, other agents via
// Provider API). Only the server knows the full picture; local JSONL misses
// non-CLI usage. alpha/usage/summary?since=<ISO> returns cumulative totals
// from that instant to now. Per-day usage = cum(day start) - cum(next day
// start).

fn iso_day_start(day: &str, tz: i64) -> String {
    if tz == 0 {
        format!("{day}T00:00:00.000Z")
    } else {
        // explicit offset: core parses it and shifts to UTC
        format!("{day}T00:00:00{}", tz_offset_suffix(tz))
    }
}

/// Civil "today" in the given fixed offset.
fn today_in_tz(tz: i64) -> String {
    if tz == 0 {
        return today_utc();
    }
    let now = now_secs() as i64;
    civil_from_days((now - tz).div_euclid(86400))
}

/// Fetch cumulative summaries for many `since` boundaries with bounded
/// concurrency; result order matches input order.
/// ponytail: 8-way pool, no semaphore crate — spawn-and-join in chunks.
/// upgrade: tune POOL only if account reports ever hit latency limits.
fn fetch_pool(sinces: &[String], key: &str) -> Result<Vec<api::UsageSummary>, String> {
    const POOL: usize = 8;
    let mut out = Vec::new();
    for chunk in sinces.chunks(POOL) {
        let handles: Vec<_> = chunk
            .iter()
            .map(|s| {
                let s = s.clone();
                let k = key.to_string();
                std::thread::spawn(move || api::summary_since(&s, &k))
            })
            .collect();
        for h in handles {
            out.push(h.join().map_err(|_| "usage thread panicked".to_string())??);
        }
    }
    Ok(out)
}

/// Account-wide per-day usage for the last `days` days (today included),
/// fetched from the usage API (8-way concurrent). Includes usage from
/// every harness that used the account key.
pub fn load_account_daily(days: usize, key: &str, tz: i64) -> Result<ByDay, String> {
    let today = today_in_tz(tz);
    let days = days.max(1);
    let days_list: Vec<String> = (0..days)
        .filter_map(|i| day_shift(&today, -(i as i64)))
        .collect();
    let sinces: Vec<String> = days_list.iter().map(|d| iso_day_start(d, tz)).collect();
    let cums = fetch_pool(&sinces, key)?;
    // zip back with day labels, sort oldest → newest
    let mut by_day: Vec<(String, api::UsageSummary)> = days_list.into_iter().zip(cums).collect();
    by_day.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = ByDay::new();
    for (i, (day, cum)) in by_day.iter().enumerate() {
        // per-day = cum(day start) - cum(next day start); for today subtract 0
        let (reqs, cost, tin, tout) = if i + 1 < by_day.len() {
            let next = &by_day[i + 1].1;
            (
                cum.total_count.saturating_sub(next.total_count),
                (cum.total_cost - next.total_cost).max(0.0),
                cum.total_tokens_in.saturating_sub(next.total_tokens_in),
                cum.total_tokens_out.saturating_sub(next.total_tokens_out),
            )
        } else {
            (
                cum.total_count,
                cum.total_cost,
                cum.total_tokens_in,
                cum.total_tokens_out,
            )
        };
        if reqs == 0 && cost == 0.0 {
            continue;
        }
        out.insert(
            day.clone(),
            Totals {
                requests: reqs,
                usage: Usage {
                    input_tokens: tin,
                    output_tokens: tout,
                    cost_usd: cost,
                    ..Default::default()
                },
            },
        );
    }
    Ok(out)
}

pub fn sum_days(by_day: &ByDay) -> Totals {
    let mut t = Totals::default();
    for v in by_day.values() {
        t.merge(v);
    }
    t
}

// ---- Hourly buckets (account-wide) ----
// Same cumulative-diff trick with hour boundaries: per-hour usage =
// cum(hour start) - cum(next hour start). Today's in-progress hour =
// cum(hour start) itself.

/// Account-wide usage for the last `hours` hours, one row per hour bucket
/// (oldest first, current hour last). Includes all harnesses.
pub fn load_account_hourly(hours: usize, key: &str, tz: i64) -> Result<Vec<(String, Totals)>, String> {
    let hours = hours.max(1);
    let now = now_secs();
    // bucket boundaries in the requested offset: shift to local, floor to the
    // hour, then shift back to UTC to form the `since` instant. Labels use the
    // local hour.
    let local_now = (now as i64 - tz) as u64;
    let current_hour_local = local_now - local_now % 3600;
    let bounds_local: Vec<u64> = (0..hours)
        .rev()
        .map(|i| current_hour_local - (i as u64) * 3600)
        .collect();
    let bounds_utc: Vec<u64> = bounds_local.iter().map(|b| (*b as i64 + tz) as u64).collect();

    // bounded 8-way pool (shared fetch_pool); cums[i] aligns with bounds[i]
    let sinces: Vec<String> = bounds_utc.iter().map(|&b| iso_hour_start(b)).collect();
    let cums = fetch_pool(&sinces, key)?;

    // cum[i] = usage from bounds[i] → now. per-hour i = cum[i] - cum[i+1];
    // current (last) bucket = cum[last] (nothing after it to subtract — it
    // covers only up to now, which is what we want).
    let mut out = Vec::new();
    for (i, b) in bounds_local.iter().enumerate() {
        let (reqs, cost, tin, tout) = if i + 1 < cums.len() {
            let next = &cums[i + 1];
            (
                cums[i].total_count.saturating_sub(next.total_count),
                (cums[i].total_cost - next.total_cost).max(0.0),
                cums[i].total_tokens_in.saturating_sub(next.total_tokens_in),
                cums[i].total_tokens_out.saturating_sub(next.total_tokens_out),
            )
        } else {
            (
                cums[i].total_count,
                cums[i].total_cost,
                cums[i].total_tokens_in,
                cums[i].total_tokens_out,
            )
        };
        out.push((
            hour_label(*b),
            Totals {
                requests: reqs,
                usage: Usage {
                    input_tokens: tin,
                    output_tokens: tout,
                    cost_usd: cost,
                    ..Default::default()
                },
            },
        ));
    }
    Ok(out)
}

/// Local hourly buckets from JSONL logs (offline, CLI sessions only).
/// Returns (label, totals) oldest-first for the last `hours` hours, UTC.
pub fn load_local_hourly(hours: usize) -> Vec<(String, Totals)> {
    let hours = hours.max(1);
    let now = now_secs();
    let current_hour = now - now % 3600;
    let oldest = current_hour - (hours as u64 - 1) * 3600;

    // timestamp ISO → hour bucket index
    let bucket_of = |ts: &str| -> Option<u64> {
        let ms: f64 = parse_iso_utc(ts)?;
        let s = (ms / 1000.0) as u64;
        let h = s - s % 3600;
        (h >= oldest).then_some(h)
    };

    let mut by_hour: BTreeMap<u64, Totals> = BTreeMap::new();
    for (_proj, text) in session_files() {
        for line in text.lines() {
            let Ok(l) = serde_json::from_str::<Line>(line) else {
                continue;
            };
            if l.kind != "message" {
                continue;
            }
            let Some(u) = l.usage else { continue };
            if l.message.as_ref().and_then(|m| m.role.as_deref()) == Some("user") {
                continue;
            }
            if let Some(h) = bucket_of(&l.timestamp) {
                by_hour.entry(h).or_default().add(&u);
            }
        }
    }
    (0..hours)
        .rev()
        .map(|i| {
            let h = current_hour - (i as u64) * 3600;
            (hour_label(h), by_hour.get(&h).copied().unwrap_or_default())
        })
        .collect()
}
