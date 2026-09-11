use super::cli::{parse_duration, parse_tz, usage_text};

#[test]
fn duration_suffixes() {
    assert_eq!(parse_duration("30"), Some(30));
    assert_eq!(parse_duration("30s"), Some(30));
    assert_eq!(parse_duration("5m"), Some(300));
    assert_eq!(parse_duration("1h"), Some(3600));
    assert_eq!(parse_duration("2d"), Some(172_800));
    assert_eq!(parse_duration("10S"), Some(10));
    assert_eq!(parse_duration(""), None);
    assert_eq!(parse_duration("abc"), None);
    assert_eq!(parse_duration("5x"), None);
}

#[test]
fn tz_offsets() {
    assert_eq!(parse_tz("+05:30"), Some(5 * 3600 + 30 * 60));
    assert_eq!(parse_tz("-08:00"), Some(-8 * 3600));
    assert_eq!(parse_tz("+0530"), Some(5 * 3600 + 30 * 60));
    assert_eq!(parse_tz("+8"), Some(8 * 3600));
    assert_eq!(parse_tz("05:30"), None); // missing sign
    assert_eq!(parse_tz("+15:00"), None); // out of range
    assert_eq!(parse_tz("+"), None);
}

#[test]
fn parse_flags() {
    // can't call parse_args (reads std::env::args), so test via usage text presence
    // and keep parse logic covered by integration runs. Sanity-check usage text:
    let u = usage_text();
    assert!(u.contains("--once"));
    assert!(u.contains("--interval"));
    assert!(u.contains("--bar-width"));
    assert!(u.contains("--plain"));
    assert!(u.contains("config set"));
    assert!(u.contains("config.json"));
}
