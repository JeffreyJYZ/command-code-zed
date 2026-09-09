use crate::api;
use crate::dates::{day_shift, iso_hour_start, hour_label, parse_iso_utc, today_utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
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
        self.requests += 1;
        self.usage.input_tokens += u.input_tokens;
        self.usage.output_tokens += u.output_tokens;
        self.usage.cache_read_tokens += u.cache_read_tokens;
        self.usage.cache_write_tokens += u.cache_write_tokens;
        self.usage.cost_usd += u.cost_usd;
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

fn data_dir() -> PathBuf {
    let home: PathBuf = std::env::var("HOME").unwrap_or_else(|_| "/".into()).into();
    home.join(".commandcode/projects")
}

/// day key = UTC YYYY-MM-DD from ISO timestamp
fn day_of(ts: &str) -> Option<String> {
    ts.get(0..10).map(|s| s.to_string())
}

pub fn load_local() -> LocalData {
    let mut data = LocalData {
        by_day: ByDay::new(),
        by_model: ByModel::new(),
        by_project: ByProject::new(),
        sessions: 0,
        total: Totals::default(),
    };

    let dir = data_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return data;
    };

    for proj in entries.flatten() {
        let proj_name = proj.file_name().to_string_lossy().to_string();
        let Ok(files) = std::fs::read_dir(proj.path()) else {
            continue;
        };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            // skip checkpoints and non-sessions
            if name.ends_with(".meta.json") || !name.ends_with(".jsonl") {
                continue;
            }
            if name.contains("checkpoints") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(f.path()) else {
                continue;
            };
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
    }
    data
}

// ---- Account-wide daily usage (API, all harnesses) ----
// The account API key can be used from any harness (CLI, other agents via
// Provider API). Only the server knows the full picture; local JSONL misses
// non-CLI usage. alpha/usage/summary?since=<ISO> returns cumulative totals
// from that instant to now. Per-day usage = cum(day start) - cum(next day
// start).

fn iso_day_start(day: &str) -> String {
    format!("{day}T00:00:00.000Z")
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
pub fn load_account_daily(days: usize, key: &str) -> Result<ByDay, String> {
    let today = today_utc();
    let days = days.max(1);
    let days_list: Vec<String> = (0..days)
        .filter_map(|i| day_shift(&today, -(i as i64)))
        .collect();
    let sinces: Vec<String> = days_list.iter().map(|d| iso_day_start(d)).collect();
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
        t.requests += v.requests;
        t.usage.input_tokens += v.usage.input_tokens;
        t.usage.output_tokens += v.usage.output_tokens;
        t.usage.cache_read_tokens += v.usage.cache_read_tokens;
        t.usage.cache_write_tokens += v.usage.cache_write_tokens;
        t.usage.cost_usd += v.usage.cost_usd;
    }
    t
}

// ---- Hourly buckets (account-wide) ----
// Same cumulative-diff trick with hour boundaries: per-hour usage =
// cum(hour start) - cum(next hour start). Today's in-progress hour =
// cum(hour start) itself.

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Account-wide usage for the last `hours` hours, one row per hour bucket
/// (oldest first, current hour last). Includes all harnesses.
pub fn load_account_hourly(hours: usize, key: &str) -> Result<Vec<(String, Totals)>, String> {
    let hours = hours.max(1);
    let now = now_epoch();
    let current_hour = now - now % 3600;
    // boundaries: start of each of the last N hours (oldest → current)
    let bounds: Vec<u64> = (0..hours)
        .rev()
        .map(|i| current_hour - (i as u64) * 3600)
        .collect();

    // bounded 8-way pool (shared fetch_pool); cums[i] aligns with bounds[i]
    let sinces: Vec<String> = bounds.iter().map(|&b| iso_hour_start(b)).collect();
    let cums = fetch_pool(&sinces, key)?;

    // cum[i] = usage from bounds[i] → now. per-hour i = cum[i] - cum[i+1];
    // current (last) bucket = cum[last] (nothing after it to subtract — it
    // covers only up to now, which is what we want).
    let mut out = Vec::new();
    for (i, b) in bounds.iter().enumerate() {
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
    let now = now_epoch();
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
    let dir = data_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    for proj in entries.flatten() {
        let Ok(files) = std::fs::read_dir(proj.path()) else {
            continue;
        };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if !name.ends_with(".jsonl") || name.contains("checkpoints") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(f.path()) else {
                continue;
            };
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
    }
    (0..hours)
        .rev()
        .map(|i| {
            let h = current_hour - (i as u64) * 3600;
            (hour_label(h), by_hour.get(&h).copied().unwrap_or_default())
        })
        .collect()
}
