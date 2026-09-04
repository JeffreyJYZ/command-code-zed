use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Config {
    pub interval_secs: u64,
    pub bar_width: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config { interval_secs: 5, bar_width: 20 }
    }
}

pub fn config_path() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(|d| PathBuf::from(d).join("cmd-usage/config.json"))
        .unwrap_or_else(|_| {
            dirs_home().join(".config/cmd-usage/config.json")
        })
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME").unwrap_or_else(|_| "/".into()).into()
}

pub fn load() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            eprintln!("warn: bad config {}: {e}; using defaults", path.display());
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

/// Persist any subset of settings; missing keys keep current values.
pub fn set(interval_secs: Option<u64>, bar_width: Option<usize>) -> Result<(), String> {
    if interval_secs.is_none() && bar_width.is_none() {
        return Err("nothing to set".into());
    }
    if let Some(v) = interval_secs {
        if v == 0 {
            return Err("interval must be ≥ 1 second".into());
        }
    }
    if let Some(v) = bar_width {
        if !(5..=200).contains(&v) {
            return Err("bar width must be 5–200".into());
        }
    }

    let cur = load();
    let cfg = Config {
        interval_secs: interval_secs.unwrap_or(cur.interval_secs),
        bar_width: bar_width.unwrap_or(cur.bar_width),
    };
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;

    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, json + "\n").map_err(|e| e.to_string())?;
    println!("saved {}", path.display());
    println!("  interval_secs = {}", cfg.interval_secs);
    println!("  bar_width     = {}", cfg.bar_width);
    Ok(())
}
