use super::config::{set, Config};
use crate::cli::ConfigSet;

fn cs(interval: Option<u64>, width: Option<usize>) -> ConfigSet {
    ConfigSet {
        interval,
        width,
        ..Default::default()
    }
}

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
    assert!(d.burst_enabled, "spend-bursts default ON");
    assert_eq!(d.burst_samples, 40);
    assert!(d.check_updates, "update check default ON");
    assert!(d.dismissed_update.is_none());
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
    assert!(c.burst_enabled);
    let c: Config =
        serde_json::from_str(r#"{"burst_enabled": false, "burst_samples": 20}"#).unwrap();
    assert!(!c.burst_enabled);
    assert_eq!(c.burst_samples, 20);
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
    set(&cs(Some(15), Some(30))).unwrap();
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(c.interval_secs, 15);
    assert_eq!(c.bar_width, 30);
    assert!(c.burst_enabled);

    // partial set keeps other key
    set(&cs(Some(60), None)).unwrap();
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(c.interval_secs, 60);
    assert_eq!(c.bar_width, 30);

    // burst toggle off keeps samples; notify toggle off persists
    set(&ConfigSet {
        burst_on: Some(false),
        notify: Some(false),
        ..Default::default()
    })
    .unwrap();
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(!c.burst_enabled);
    assert!(!c.notify_on_cap);

    // update toggle + dismissed version persist
    set(&ConfigSet {
        update_check: Some(false),
        dismissed_update: Some("0.6.8".into()),
        ..Default::default()
    })
    .unwrap();
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(!c.check_updates);
    assert_eq!(c.dismissed_update.as_deref(), Some("0.6.8"));

    // set_dismissed keeps unrelated settings
    super::config::set_dismissed("9.9.9").unwrap();
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(c.dismissed_update.as_deref(), Some("9.9.9"));
    assert!(!c.check_updates);

    // validation errors
    assert!(set(&cs(Some(0), None)).is_err());
    assert!(set(&cs(Some(86_401), None)).is_err());
    assert!(set(&cs(None, Some(4))).is_err()); // < 5
    assert!(set(&cs(None, Some(201))).is_err()); // > 200
    assert!(set(&ConfigSet {
        bursts: Some(3),
        ..Default::default()
    })
    .is_err()); // < 5
    assert!(set(&ConfigSet {
        bursts: Some(241),
        ..Default::default()
    })
    .is_err()); // > 240
    assert!(set(&ConfigSet::default()).is_err()); // nothing to set

    // file unchanged after failed set
    let c: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(c.interval_secs, 60);
    assert!(!c.burst_enabled);
    assert!(!c.notify_on_cap);

    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("XDG_CONFIG_HOME");
}
