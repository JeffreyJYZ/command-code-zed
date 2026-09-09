use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Config {
    pub interval_secs: u64,
    pub bar_width: usize,
    pub statusline_template: String,
    pub statusline_colors: bool,
    pub statusline_ascii: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            interval_secs: 5,
            bar_width: 20,
            statusline_template: "{plan} {credits}/{cap} \u{b7} 5h {5h_bar} \u{b7} wk {wk_bar}".into(),
            statusline_colors: true,
            statusline_ascii: false,
        }
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
pub fn set(
    interval_secs: Option<u64>,
    bar_width: Option<usize>,
    sl_template: Option<String>,
    sl_colors: Option<bool>,
    sl_ascii: Option<bool>,
) -> Result<(), String> {
    if interval_secs.is_none()
        && bar_width.is_none()
        && sl_template.is_none()
        && sl_colors.is_none()
        && sl_ascii.is_none()
    {
        return Err("nothing to set".into());
    }
    if let Some(v) = interval_secs {
        if v == 0 || v > 86_400 {
            return Err("interval must be 1–86400 seconds".into());
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
        statusline_template: sl_template.unwrap_or_else(|| cur.statusline_template.clone()),
        statusline_colors: sl_colors.unwrap_or(cur.statusline_colors),
        statusline_ascii: sl_ascii.unwrap_or(cur.statusline_ascii),
    };
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;

    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, json + "\n").map_err(|e| e.to_string())?;
    println!("saved {}", path.display());
    println!("  interval_secs       = {}", cfg.interval_secs);
    println!("  bar_width           = {}", cfg.bar_width);
    println!("  statusline_template = {}", cfg.statusline_template);
    println!("  statusline_colors   = {}", cfg.statusline_colors);
    println!("  statusline_ascii    = {}", cfg.statusline_ascii);
    Ok(())
}
