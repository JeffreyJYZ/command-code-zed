use super::config::{set, Config};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("cmduse-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn config_defaults_when_missing() {
    // load() falls back to defaults when file absent — hard to inject path
    // without env var, so test the Default impl + parse round-trip instead
    let d = Config::default();
    assert_eq!(d.interval_secs, 5);
    assert_eq!(d.bar_width, 20);
}

#[test]
fn config_parse_valid() {
    let c: Config = serde_json::from_str(r#"{"interval_secs": 30, "bar_width": 40}"#).unwrap();
    assert_eq!(c.interval_secs, 30);
    assert_eq!(c.bar_width, 40);
}

#[test]
fn config_parse_partial_uses_defaults() {
    let c: Config = serde_json::from_str(r#"{"interval_secs": 30}"#).unwrap();
    assert_eq!(c.interval_secs, 30);
    assert_eq!(c.bar_width, 20); // serde(default) fills
    let c: Config = serde_json::from_str(r#"{"bar_width": 9}"#).unwrap();
    assert_eq!(c.interval_secs, 5);
}

#[test]
fn config_parse_garbage_rejected() {
    assert!(serde_json::from_str::<Config>("not json").is_err());
    assert!(serde_json::from_str::<Config>("{\"interval_secs\": \"abc\"}").is_err());
}

#[test]
fn config_set_validates_and_persists() {
    let dir = temp_dir("set");
    let path = dir.join("cmd-usage/config.json");
    std::env::set_var("XDG_CONFIG_HOME", &dir);

    // valid set
    set(Some(15), Some(30)).unwrap();
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(c.interval_secs, 15);
    assert_eq!(c.bar_width, 30);

    // partial set keeps other key
    set(Some(60), None).unwrap();
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(c.interval_secs, 60);
    assert_eq!(c.bar_width, 30);

    // validation errors
    assert!(set(Some(0), None).is_err());
    assert!(set(Some(86_401), None).is_err());
    assert!(set(None, Some(4)).is_err()); // < 5
    assert!(set(None, Some(201)).is_err()); // > 200
    assert!(set(None, None).is_err()); // nothing to set

    // file unchanged after failed set
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(c.interval_secs, 60);

    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("XDG_CONFIG_HOME");
}
