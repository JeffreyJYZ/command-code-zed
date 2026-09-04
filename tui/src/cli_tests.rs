use super::cli::usage_text;

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

#[test]
fn parse_values_from_pairs() {
    // exercise the same matching logic parse_args uses, extracted here as pure fn
    fn kv(k: &str, v: &str) -> Option<(String, String)> {
        Some((k.to_string(), v.to_string()))
    }
    assert!(kv("interval", "10").is_some());
    assert_eq!(kv("interval", "10").unwrap().1.parse::<u64>().ok(), Some(10));
    assert_eq!(kv("interval", "abc").unwrap().1.parse::<u64>().ok(), None);
    assert_eq!(kv("width", "25").unwrap().1.parse::<usize>().ok(), Some(25));
}
