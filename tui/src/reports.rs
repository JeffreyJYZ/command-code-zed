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

#[derive(Deserialize)]
struct ApiSummary {
    #[serde(default, rename = "totalCount")]
    total_count: u64,
    #[serde(default, rename = "totalCost")]
    total_cost: f64,
    #[serde(default, rename = "totalTokensIn")]
    total_tokens_in: u64,
    #[serde(default, rename = "totalTokensOut")]
    total_tokens_out: u64,
}

fn iso_day_start(day: &str) -> String {
    format!("{day}T00:00:00.000Z")
}

fn fetch_cumulative(since: &str, key: &str) -> Result<ApiSummary, String> {
    let url = format!("https://api.commandcode.ai/alpha/usage/summary?since={since}");
    let get = || -> Result<ureq::Response, String> {
        ureq::get(&url)
            .set("Authorization", &format!("Bearer {key}"))
            .timeout(std::time::Duration::from_secs(15))
            .call()
            .map_err(|e| e.to_string())
    };
    // one retry on transient connection failures (box the big ureq error)
    let resp = match get() {
        Ok(r) => r,
        Err(e) => {
            // one retry on transient connection failures
            get().map_err(|e2| format!("summary since={since}: {e} / {e2}"))?
        }
    };
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .take(1 << 20)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| format!("summary parse: {e}"))
}

/// today as UTC YYYY-MM-DD (no chrono; days-since-epoch → civil date)
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86400;
    // Howard Hinnant civil_from_days
    let z = days as i64 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// shift YYYY-MM-DD by n days (UTC)
pub fn day_shift(day: &str, n: i64) -> Option<String> {
    let mut p = day.split('-');
    let y: i64 = p.next()?.parse().ok()?;
    let m: i64 = p.next()?.parse().ok()?;
    let d: i64 = p.next()?.parse().ok()?;
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468 + n;
    // back to civil
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// Account-wide per-day usage for the last `days` days (today included),
/// fetched in parallel from the usage API. Includes usage from every
/// harness that used the account key.
pub fn load_account_daily(days: usize, key: &str) -> Result<ByDay, String> {
    let today = today_utc();
    // day boundaries needed: start of each day + start of "tomorrow" (=now, cum 0 conceptually;
    // actually today's usage = cum(today start))
    let days = days.max(1);
    let starts: Vec<String> = (0..days)
        .filter_map(|i| day_shift(&today, -(i as i64)))
        .collect();

    let handles: Vec<_> = starts
        .iter()
        .map(|d| {
            let s = iso_day_start(d);
            let k = key.to_string();
            let dd = d.clone();
            std::thread::spawn(move || (dd, fetch_cumulative(&s, &k)))
        })
        .collect();

    // collect in chronological order (oldest first)
    let mut cums: Vec<(String, ApiSummary)> = Vec::new();
    for h in handles {
        let (d, r) = h.join().map_err(|_| "usage thread panicked".to_string())?;
        cums.push((d, r?));
    }
    // sort oldest → newest
    cums.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = ByDay::new();
    for (i, (day, cum)) in cums.iter().enumerate() {
        // per-day = cum(day start) - cum(next day start); for today subtract 0
        let (reqs, cost, tin, tout) = if i + 1 < cums.len() {
            let next = &cums[i + 1].1;
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

/// hour start epoch → ISO with ms
fn iso_hour_start(epoch: u64) -> String {
    let hour_start = epoch - epoch % 3600;
    // civil from epoch (reuse day logic inline)
    let days = hour_start / 86400;
    let secs_of_day = hour_start % 86400;
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    // reuse day_shift's civil math by constructing date from days-since-epoch
    let date = civil_from_days(days as i64);
    format!("{date}T{h:02}:{m:02}:{s:02}.000Z")
}

fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// label for an hour bucket: "MM-DD HH:00"
fn hour_label(epoch: u64) -> String {
    let hour_start = epoch - epoch % 3600;
    let days = hour_start / 86400;
    let h = (hour_start % 86400) / 3600;
    format!("{} {h:02}:00", &civil_from_days(days as i64)[5..])
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

    let handles: Vec<_> = bounds
        .iter()
        .map(|&b| {
            let s = iso_hour_start(b);
            let k = key.to_string();
            std::thread::spawn(move || fetch_cumulative(&s, &k))
        })
        .collect();

    let mut cums: Vec<ApiSummary> = Vec::new();
    for h in handles {
        cums.push(h.join().map_err(|_| "usage thread panicked".to_string())??);
    }

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
