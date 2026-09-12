use super::cli::{parse_args_from, parse_duration, parse_tz, usage_text, SubCmd};

fn argv(args: &[&str]) -> Vec<std::ffi::OsString> {
    // lexopt::from_iter wants the binary name first, like env::args_os.
    std::iter::once(std::ffi::OsString::from("cmduse"))
        .chain(args.iter().map(std::ffi::OsString::from))
        .collect()
}

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
fn parses_flags_all_forms() {
    let a = parse_args_from(argv(&["-1", "-i", "30s", "--tz", "+05:30", "--json"])).unwrap();
    assert!(a.once);
    assert_eq!(a.interval, Some(30));
    assert_eq!(a.tz, Some(19_800));
    assert!(a.json);
    // --key=value and attached short value both work through lexopt
    let a = parse_args_from(argv(&["--interval=5m", "-w40"])).unwrap();
    assert_eq!(a.interval, Some(300));
    assert_eq!(a.bar_width, Some(40));
    let a = parse_args_from(argv(&["daily", "--days", "3"])).unwrap();
    assert_eq!(a.subcmd, Some(SubCmd::Daily));
    assert_eq!(a.last, Some(3));
}

#[test]
fn parses_config_set() {
    let a = parse_args_from(argv(&["config", "set", "interval=10", "sl_colors=false"])).unwrap();
    let cs = a.config_set.unwrap();
    assert_eq!(cs.interval, Some(10));
    assert_eq!(cs.sl_colors, Some(false));
}

#[test]
fn rejects_bad_input() {
    assert!(parse_args_from(argv(&["-1", "-W"])).is_err());
    assert!(parse_args_from(argv(&["--tz", "junk"])).is_err());
    assert!(parse_args_from(argv(&["-w", "3"])).is_err());
    assert!(parse_args_from(argv(&["config", "set", "width=abc"])).is_err());
    assert!(parse_args_from(argv(&["bogus"])).is_err());
}

#[test]
fn parse_flags() {
    // usage text stays in sync with the parser's flags
    let u = usage_text();
    assert!(u.contains("--once"));
    assert!(u.contains("--interval"));
    assert!(u.contains("--bar-width"));
    assert!(u.contains("--plain"));
    assert!(u.contains("config set"));
    assert!(u.contains("config.json"));
}
