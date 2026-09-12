// ISO date/time helpers — no chrono dependency.
//
// Parse "2026-09-27T12:23:00.000Z" (UTC), "…+05:00", or "…-07:30" → epoch
// ms. No offset suffix is treated as UTC. Lenient: no range validation.
pub fn parse_iso_utc(s: &str) -> Option<f64> {
    let (date, rest) = s.split_once('T')?;
    let rest = rest.trim_end_matches('Z');
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;
    // days since epoch via civil-from-days algorithm (Howard Hinnant)
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400;
    // limit 3 so a ±HH:MM offset keeps its own colon (a plain split(':'))
    // would swallow it: "00:00+07:30" → token "00+07", losing the :30.
    let mut hp = rest.splitn(3, ':');
    let h: i64 = hp.next().unwrap_or("0").parse().ok()?;
    let mi: i64 = hp.next().unwrap_or("0").parse().ok()?;
    let mut sec_part = hp.next().unwrap_or("0");
    let mut offset_secs = 0i64;
    if let Some(idx) = sec_part.find(['+', '-']) {
        let (secs_part, off) = sec_part.split_at(idx);
        offset_secs = parse_offset(off)?;
        sec_part = secs_part;
    }
    let sec: f64 = sec_part.parse().ok()?;
    // interpret civil time as UTC, then subtract the offset: 12:00+05:00
    // means 07:00 UTC, i.e. 5h earlier than the literal civil read.
    Some((secs + h * 3600 + mi * 60) as f64 * 1000.0 + sec * 1000.0 - offset_secs as f64 * 1000.0)
}

/// "+05:30" / "-07:30" / "+0530" / "+8" → (sign, hours, minutes). No range
/// validation — `parse_iso_utc` stays lenient; callers that need bounds
/// (e.g. the CLI's `--tz`) validate the parts themselves.
pub fn parse_tz_parts(s: &str) -> Option<(i64, i64, i64)> {
    let rest = s.strip_prefix(['-', '+'])?;
    if rest.is_empty() {
        return None;
    }
    let sign = if s.starts_with('-') { -1i64 } else { 1i64 };
    let (h, m) = match rest.split_once(':') {
        Some((h, m)) => (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?),
        None if rest.len() == 4 => {
            // basic format "+0500" / "-0730": split HHMM at the midpoint.
            let (h, m) = rest.split_at(2);
            (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?)
        }
        None => (rest.parse::<i64>().ok()?, 0),
    };
    Some((sign, h, m))
}

fn parse_offset(s: &str) -> Option<i64> {
    let (sign, h, m) = parse_tz_parts(s)?;
    Some(sign * (h * 3600 + m * 60))
}

/// days since epoch → YYYY-MM-DD (UTC). Howard Hinnant civil_from_days.
pub fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
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

/// Current epoch seconds (0 if the clock is before the epoch).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// exact epoch seconds → "YYYY-MM-DDTHH:MM:SS.000Z" (no hour flooring; use for
/// timezone-local instants that don't land on a UTC hour boundary).
pub fn iso_instant(epoch: u64) -> String {
    let secs = epoch as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    format!(
        "{}T{:02}:{:02}:{:02}.000Z",
        civil_from_days(days),
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
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
    Some(civil_from_days(days))
}

/// "MM-DD HH:00" label for an hour bucket
pub fn hour_label(epoch: u64) -> String {
    let hour_start = epoch - epoch % 3600;
    let days = hour_start as i64 / 86400;
    let h = (hour_start % 86400) / 3600;
    format!("{} {h:02}:00", &civil_from_days(days)[5..])
}

/// UTC-offset seconds east → "-08:00" / "+05:30".
pub fn tz_offset_suffix(tz_secs: i64) -> String {
    let sign = if tz_secs < 0 { '-' } else { '+' };
    let a = tz_secs.abs();
    format!("{sign}{:02}:{:02}", a / 3600, (a % 3600) / 60)
}
