use std::path::PathBuf;

const CRATE: &str = "cmd-usage";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

fn cache_path() -> PathBuf {
    let home: PathBuf = std::env::var("HOME").unwrap_or_else(|_| "/".into()).into();
    home.join(".cache/cmd-usage/last-check")
}

/// Check for update synchronously (up to 5s). Returns update message or None.
pub fn check_sync() -> Option<String> {
    if let Ok(meta) = std::fs::metadata(cache_path()) {
        if let Ok(age) = meta.modified().map(|m| m.elapsed()) {
            if age.is_ok_and(|a| a.as_secs() < 86_400) {
                return None;
            }
        }
    }
    if let Some(dir) = cache_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(cache_path(), now_secs().to_string());

    let url = format!("https://crates.io/api/v1/crates/{CRATE}");
    let resp = ureq::get(&url)
        .set("User-Agent", &format!("cmduse/{CURRENT}"))
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .ok()?;
    let json = resp.into_json::<serde_json::Value>().ok()?;
    let latest = json["crate"]["max_version"].as_str()?;
    if newer(latest, CURRENT) {
        Some(format!(
            "\x1b[33mcmduse: update available {CURRENT} → {latest} (cargo install cmd-usage / brew upgrade jeffreyjyz/tap/cmduse)\x1b[0m"
        ))
    } else {
        None
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|p| p.trim().parse().unwrap_or(0))
            .collect()
    };
    parse(latest) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("0.1.9", "0.1.10"));
        assert!(!newer("0.1.9", "0.1.9"));
    }
}
