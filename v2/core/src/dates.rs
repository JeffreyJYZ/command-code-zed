// ISO date/time helpers — no chrono dependency. All time treated as UTC
// (no offset parsing) — off by hours at most for the billing window math.

/// Parse "2026-09-27T12:23:00.000Z" → epoch ms. Lenient: no range validation.
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
    let mut hp = rest.split(':');
    let h: i64 = hp.next().unwrap_or("0").parse().ok()?;
    let mi: i64 = hp.next().unwrap_or("0").parse().ok()?;
    let sec: f64 = hp.next().unwrap_or("0").parse().ok()?;
    Some((secs + h * 3600 + mi * 60) as f64 * 1000.0 + sec * 1000.0)
}

/// days since epoch → YYYY-MM-DD (UTC). Howard Hinnant civil_from_days.
pub fn civil_from_days(days: i64) -> String {
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

/// today as UTC YYYY-MM-DD
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_days(secs as i64 / 86400)
}

/// hour boundary epoch → "YYYY-MM-DDTHH:00:00.000Z"
pub fn iso_hour_start(epoch: u64) -> String {
    let hour_start = epoch - epoch % 3600;
    let days = hour_start as i64 / 86400;
    let h = (hour_start % 86400) / 3600;
    format!("{}T{h:02}:00:00.000Z", civil_from_days(days))
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
