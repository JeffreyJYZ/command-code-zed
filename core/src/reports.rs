//! Usage aggregation — pure math for the daily/hourly/local reports.
//!
//! Lives in core, not the CLI: bucketing (day/hour in a fixed UTC offset) and
//! the cumulative-difference trick for the account API are logic, not
//! presentation, and this is where the `--tz` sign bugs bit before (see
//! `day_of`/`date_in_tz` tests). The CLI keeps only I/O: walking session
//! JSONL, the HTTP pool, and formatting.
use crate::dates::{civil_from_days, day_shift, hour_label, iso_instant, parse_iso_utc};
use crate::wire::UsageSummary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize, Clone, Copy, Default, Debug, PartialEq)]
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

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Totals {
    pub requests: u64,
    pub usage: Usage,
}

impl Totals {
    pub fn add(&mut self, u: &Usage) {
        self.merge(&Totals {
            requests: 1,
            usage: *u,
        });
    }

    pub fn merge(&mut self, o: &Totals) {
        self.requests += o.requests;
        self.usage.input_tokens += o.usage.input_tokens;
        self.usage.output_tokens += o.usage.output_tokens;
        self.usage.cache_read_tokens += o.usage.cache_read_tokens;
        self.usage.cache_write_tokens += o.usage.cache_write_tokens;
        self.usage.cost_usd += o.usage.cost_usd;
    }
}

/// day → totals
pub type ByDay = BTreeMap<String, Totals>;
/// model → totals
pub type ByModel = BTreeMap<String, Totals>;
/// project (dir name) → totals
pub type ByProject = BTreeMap<String, Totals>;

pub struct LocalData {
    pub by_day: ByDay,
    pub by_model: ByModel,
    pub by_project: ByProject,
    pub total: Totals,
}

/// One usage-bearing message, already parsed by the caller (the CLI owns the
/// JSONL walk). `project` is the session directory name. Owned fields: records
/// outlive the parsed line they came from.
pub struct UsageRecord {
    pub project: String,
    pub timestamp: String,
    pub model: Option<String>,
    pub usage: Usage,
    /// True when the record belongs to a real session file (not a stray
    /// checkpoint/metadata entry): only those count toward `by_project`.
    pub is_session: bool,
}

/// Date at epoch `now` in `tz` seconds east of UTC (local = UTC + tz).
pub fn date_in_tz(now: i64, tz: i64) -> String {
    civil_from_days((now + tz).div_euclid(86_400))
}

/// Day key (YYYY-MM-DD) for an ISO timestamp in `tz` seconds east of UTC.
/// Falls back to the raw UTC date slice when the timestamp doesn't parse.
pub fn day_of(ts: &str, tz: i64) -> Option<String> {
    match parse_iso_utc(ts) {
        Some(ms) => Some(date_in_tz((ms / 1000.0) as i64, tz)),
        None => ts.get(0..10).map(|s| s.to_string()),
    }
}

/// UTC instant for local midnight of civil `day` in `tz` seconds east of UTC.
/// Emitted as a `Z` instant, never an offset suffix: the value goes into a
/// `?since=` query and `+` would decode as a space server-side.
pub fn iso_day_start(day: &str, tz: i64) -> String {
    match parse_iso_utc(&format!("{day}T00:00:00.000Z")) {
        Some(ms) => iso_instant(((ms / 1000.0) as i64 - tz).max(0) as u64),
        None => format!("{day}T00:00:00.000Z"),
    }
}

/// Civil "today" in the given fixed offset.
pub fn today_in_tz(tz: i64) -> String {
    date_in_tz(crate::dates::now_secs() as i64, tz)
}

/// Start of the local hour containing `epoch`, `tz` seconds east of UTC.
pub fn local_hour_start(epoch: u64, tz: i64) -> u64 {
    let local = (epoch as i64 + tz) as u64;
    local - local % 3600
}

/// Bucket boundaries for the last `hours` whole hours: local bucket starts
/// (for labels) and matching UTC `since` instants (for the API). local = UTC + tz.
pub fn hour_bounds(now: u64, hours: usize, tz: i64) -> (Vec<u64>, Vec<u64>) {
    let local_now = (now as i64 + tz) as u64;
    let current_hour_local = local_now - local_now % 3600;
    let local: Vec<u64> = (0..hours)
        .rev()
        .map(|i| current_hour_local - (i as u64) * 3600)
        .collect();
    let utc: Vec<u64> = local.iter().map(|&b| (b as i64 - tz) as u64).collect();
    (local, utc)
}

/// Fold parsed records into day/model/project buckets plus a grand total.
pub fn bucket_records(records: &[UsageRecord], tz: i64) -> LocalData {
    let mut data = LocalData {
        by_day: ByDay::new(),
        by_model: ByModel::new(),
        by_project: ByProject::new(),
        total: Totals::default(),
    };
    for r in records {
        if let Some(day) = day_of(&r.timestamp, tz) {
            data.by_day.entry(day).or_default().add(&r.usage);
        }
        if let Some(m) = &r.model {
            data.by_model.entry(m.clone()).or_default().add(&r.usage);
        }
        if r.is_session {
            data.by_project
                .entry(r.project.clone())
                .or_default()
                .add(&r.usage);
        }
        data.total.add(&r.usage);
    }
    data
}

pub fn sum_days(by_day: &ByDay) -> Totals {
    let mut t = Totals::default();
    for v in by_day.values() {
        t.merge(v);
    }
    t
}

/// Cumulative-difference trick for the account API: `cums[i]` is usage from
/// boundary `i` until now, so per-bucket `i` = `cums[i] - cums[i+1]`; the last
/// (in-progress) bucket is `cums[last]` itself. Empty buckets (no requests,
/// no cost) are dropped. `labels` and `cums` must be aligned and oldest-first.
fn per_bucket(cums: &[UsageSummary], i: usize) -> Totals {
    let cur = &cums[i];
    let (reqs, cost, tin, tout) = match cums.get(i + 1) {
        Some(next) => (
            cur.total_count
                .unwrap_or(0)
                .saturating_sub(next.total_count.unwrap_or(0)),
            (cur.total_cost.unwrap_or(0.0) - next.total_cost.unwrap_or(0.0)).max(0.0),
            cur.total_tokens_in
                .unwrap_or(0)
                .saturating_sub(next.total_tokens_in.unwrap_or(0)),
            cur.total_tokens_out
                .unwrap_or(0)
                .saturating_sub(next.total_tokens_out.unwrap_or(0)),
        ),
        None => (
            cur.total_count.unwrap_or(0),
            cur.total_cost.unwrap_or(0.0),
            cur.total_tokens_in.unwrap_or(0),
            cur.total_tokens_out.unwrap_or(0),
        ),
    };
    Totals {
        requests: reqs,
        usage: Usage {
            input_tokens: tin,
            output_tokens: tout,
            cost_usd: cost,
            ..Default::default()
        },
    }
}

/// Account-wide per-day totals from aligned day labels + cumulative summaries.
pub fn daily_from_cumulative(labels: &[String], cums: &[UsageSummary]) -> ByDay {
    let mut out = ByDay::new();
    for (i, label) in labels.iter().enumerate() {
        if i >= cums.len() {
            break;
        }
        let t = per_bucket(cums, i);
        if t.requests == 0 && t.usage.cost_usd == 0.0 {
            continue;
        }
        out.insert(label.clone(), t);
    }
    out
}

/// Account-wide per-hour totals: one row per bucket (oldest first), including
/// empty hours so the report keeps a stable shape.
pub fn hourly_from_cumulative(
    bounds_local: &[u64],
    cums: &[UsageSummary],
) -> Vec<(String, Totals)> {
    bounds_local
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let t = if i < cums.len() {
                per_bucket(cums, i)
            } else {
                Totals::default()
            };
            (hour_label(*b), t)
        })
        .collect()
}

/// Day labels for the last `days` days (today included), oldest first.
pub fn recent_days(today: &str, days: usize) -> Vec<String> {
    (0..days.max(1))
        .filter_map(|i| day_shift(today, -(i as i64)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_in_tz_sign_is_east_positive() {
        // 2026-09-27T20:00:00Z
        let now = 1_790_539_200i64;
        assert_eq!(date_in_tz(now, 0), "2026-09-27");
        assert_eq!(date_in_tz(now, 19_800), "2026-09-28"); // +05:30
        assert_eq!(date_in_tz(now, -28_800), "2026-09-27"); // -08:00 → noon
        assert_eq!(date_in_tz(now, 28_800), "2026-09-28"); // +08:00 → 04:00
    }

    #[test]
    fn local_day_of_honors_tz() {
        let ts = "2026-09-27T20:00:00.000Z";
        assert_eq!(day_of(ts, 0).as_deref(), Some("2026-09-27"));
        assert_eq!(day_of(ts, 19_800).as_deref(), Some("2026-09-28")); // +05:30
        assert_eq!(day_of("garbage", 0), None);
    }

    #[test]
    fn local_hour_start_honors_tz() {
        // 20:00Z with +05:30 → 01:30 local on the next day → hour start 01:00.
        let e = 1_790_539_200u64;
        assert_eq!(local_hour_start(e, 0), 1_790_539_200);
        assert_eq!(local_hour_start(e, 19_800), 1_790_557_200);
    }

    #[test]
    fn iso_day_start_is_utc_z_instant() {
        // +05:30 local midnight = 18:30 UTC the previous day; never a '+'
        // suffix (would decode as a space in the API query string).
        assert_eq!(
            iso_day_start("2026-09-28", 19_800),
            "2026-09-27T18:30:00.000Z"
        );
        assert_eq!(iso_day_start("2026-09-28", 0), "2026-09-28T00:00:00.000Z");
        assert_eq!(
            iso_day_start("2026-09-28", -28_800),
            "2026-09-28T08:00:00.000Z"
        );
        assert!(!iso_day_start("2026-09-28", 19_800).contains('+'));
    }

    #[test]
    fn hour_bounds_align_utc_since_to_local_hour() {
        // 2026-09-27T20:00:00Z with +05:30 → local 01:30 on 09-28.
        let (local, utc) = hour_bounds(1_790_539_200, 2, 19_800);
        assert_eq!(local, vec![1_790_553_600, 1_790_557_200]);
        assert_eq!(utc, vec![1_790_533_800, 1_790_537_400]);
        // minute-bearing offset must land on :30 UTC, not be hour-floored
        assert_eq!(iso_instant(utc[1]), "2026-09-27T19:30:00.000Z");
    }

    fn summary(count: u64, cost: f64, tin: u64, tout: u64) -> UsageSummary {
        UsageSummary {
            total_count: Some(count),
            total_cost: Some(cost),
            total_tokens_in: Some(tin),
            total_tokens_out: Some(tout),
            ..Default::default()
        }
    }

    #[test]
    fn daily_cumulative_difference_and_empty_bucket_drop() {
        // newest-first cumulative chain, displayed oldest-first after zip+sort
        let labels = vec!["2026-09-26".to_string(), "2026-09-27".to_string()];
        let cums = vec![summary(10, 5.0, 100, 10), summary(4, 2.0, 40, 4)];
        let by_day = daily_from_cumulative(&labels, &cums);
        let day26 = by_day.get("2026-09-26").expect("day 26");
        assert_eq!(day26.requests, 6);
        assert_eq!(day26.usage.cost_usd, 3.0);
        assert_eq!(day26.usage.input_tokens, 60);
        let day27 = by_day
            .get("2026-09-27")
            .expect("day 27 (in progress = cum)");
        assert_eq!(day27.requests, 4);
        assert_eq!(day27.usage.cost_usd, 2.0);
    }

    #[test]
    fn hourly_keeps_empty_buckets() {
        let bounds = vec![1_790_553_600u64, 1_790_557_200u64];
        let cums = vec![summary(3, 1.0, 30, 3), summary(1, 0.5, 10, 1)];
        let rows = hourly_from_cumulative(&bounds, &cums);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1.requests, 2);
        assert_eq!(rows[1].1.requests, 1);
        // aligned yet empty cumulative list still yields labelled zero rows
        let zeros = hourly_from_cumulative(&bounds, &[]);
        assert_eq!(zeros.len(), 2);
        assert_eq!(zeros[0].1.requests, 0);
    }

    #[test]
    fn bucket_records_splits_day_model_project() {
        let u = Usage {
            input_tokens: 5,
            cost_usd: 0.5,
            ..Default::default()
        };
        let records = [
            UsageRecord {
                project: "proj-a".to_string(),
                timestamp: "2026-09-27T20:00:00.000Z".to_string(),
                model: Some("m1".to_string()),
                usage: u,
                is_session: true,
            },
            UsageRecord {
                project: "proj-a".to_string(),
                timestamp: "2026-09-27T21:00:00.000Z".to_string(),
                model: None,
                usage: u,
                is_session: false,
            },
        ];
        let d = bucket_records(&records, 19_800); // +05:30 → both land on 09-28
        assert_eq!(d.by_day.keys().collect::<Vec<_>>(), vec!["2026-09-28"]);
        assert_eq!(d.by_day["2026-09-28"].requests, 2);
        assert_eq!(d.by_model.len(), 1);
        assert_eq!(d.by_project.len(), 1); // only the session record
        assert_eq!(d.by_project["proj-a"].requests, 1);
        assert_eq!(d.total.requests, 2);
    }
}
