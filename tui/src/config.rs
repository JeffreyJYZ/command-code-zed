use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Clone)]
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
