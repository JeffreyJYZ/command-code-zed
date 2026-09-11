use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Config {
    pub interval_secs: u64,
    pub bar_width: usize,
    pub burst_enabled: bool,
    pub burst_samples: usize,
    pub notify_on_cap: bool,
    pub statusline_template: String,
    pub statusline_colors: bool,
    pub statusline_ascii: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            interval_secs: 5,
            bar_width: 20,
            burst_enabled: true,
            burst_samples: 40,
            notify_on_cap: true,
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
            crate::paths::home().join(".config/cmd-usage/config.json")
        })
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
pub fn set(cs: &crate::cli::ConfigSet) -> Result<(), String> {
    if [
        cs.interval.is_some(),
        cs.width.is_some(),
        cs.sl_template.is_some(),
        cs.sl_colors.is_some(),
        cs.sl_ascii.is_some(),
        cs.burst_on.is_some(),
        cs.bursts.is_some(),
        cs.notify.is_some(),
    ]
    .iter()
    .all(|set| !set)
    {
        return Err("nothing to set".into());
    }
    if let Some(v) = cs.interval {
        if v == 0 || v > 86_400 {
            return Err("interval must be 1–86400 seconds".into());
        }
    }
    if let Some(v) = cs.width {
        if !(5..=200).contains(&v) {
            return Err("bar width must be 5–200".into());
        }
    }
    if let Some(v) = cs.bursts {
        if !(5..=240).contains(&v) {
            return Err("burst samples must be 5–240".into());
        }
    }

    let cur = load();
    let cfg = Config {
        interval_secs: cs.interval.unwrap_or(cur.interval_secs),
        bar_width: cs.width.unwrap_or(cur.bar_width),
        burst_enabled: cs.burst_on.unwrap_or(cur.burst_enabled),
        burst_samples: cs.bursts.unwrap_or(cur.burst_samples),
        notify_on_cap: cs.notify.unwrap_or(cur.notify_on_cap),
        statusline_template: cs.sl_template.clone().unwrap_or(cur.statusline_template),
        statusline_colors: cs.sl_colors.unwrap_or(cur.statusline_colors),
        statusline_ascii: cs.sl_ascii.unwrap_or(cur.statusline_ascii),
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
    println!("  burst_enabled       = {}", cfg.burst_enabled);
    println!("  burst_samples       = {}", cfg.burst_samples);
    println!("  notify_on_cap       = {}", cfg.notify_on_cap);
    println!("  statusline_template = {}", cfg.statusline_template);
    println!("  statusline_colors   = {}", cfg.statusline_colors);
    println!("  statusline_ascii    = {}", cfg.statusline_ascii);
    Ok(())
}
