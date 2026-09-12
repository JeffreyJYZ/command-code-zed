use std::path::PathBuf;

const CRATE: &str = "cmd-usage";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const MAX_AGE_SECS: u64 = 86_400;

fn cache_path() -> PathBuf {
    crate::paths::home().join(".cache/cmd-usage/last-check")
}
/// Latest version known from a fresh cache, else fetched from crates.io (and
/// cached for 24h). None on network/parse failure.
fn latest_version() -> Option<String> {
    let now = cmduse_core::dates::now_secs();
    // Fresh cache: no network, just replay the last known version.
    if let Ok(text) = std::fs::read_to_string(cache_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let (Some(checked), Some(latest)) = (v["checkedAt"].as_u64(), v["latest"].as_str()) {
                if now.saturating_sub(checked) < MAX_AGE_SECS {
                    return Some(latest.to_string());
                }
            }
        }
    }
    if let Some(dir) = cache_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let url = format!("https://crates.io/api/v1/crates/{CRATE}");
    let resp = ureq::get(&url)
        .set("User-Agent", &format!("cmduse/{CURRENT}"))
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .ok()?;
    let json = resp.into_json::<serde_json::Value>().ok()?;
    // Prefer max_stable_version: max_version includes pre-releases, which must
    // not prompt an "update available" for a stable release.
    let latest = json["crate"]["max_stable_version"]
        .as_str()
        .or_else(|| json["crate"]["max_version"].as_str())?;
    // Cache only after a successful check: an offline/transient failure must
    // not silence the next run for 24h. Old-format caches (bare timestamp)
    // fail the JSON parse above and are replaced here.
    let entry = serde_json::json!({ "checkedAt": now, "latest": latest });
    let _ = std::fs::write(cache_path(), entry.to_string());
    Some(latest.to_string())
}

/// Warning line when checking is enabled and the latest version is neither the
/// one the user dismissed nor the running version.
pub fn check_sync(enabled: bool, dismissed: Option<&str>) -> Option<String> {
    if !enabled {
        return None;
    }
    let latest = latest_version()?;
    if Some(latest.as_str()) == dismissed {
        return None;
    }
    update_msg(&latest)
}

/// Newer-than-CURRENT version for `--dismiss-update`; `Ok(None)` when up to
/// date, `Err` when crates.io couldn't be reached.
pub fn latest_newer() -> Result<Option<String>, String> {
    match latest_version() {
        Some(v) if newer(&v, CURRENT) => Ok(Some(v)),
        Some(_) => Ok(None),
        None => Err("could not check crates.io for the latest version".into()),
    }
}

/// Plain "update available" line when `latest` beats CURRENT; the caller
/// colors/places it (frame in watch mode, stderr for one-shot).
fn update_msg(latest: &str) -> Option<String> {
    if !newer(latest, CURRENT) {
        return None;
    }
    Some(format!(
        "update available {CURRENT} → {latest} (cargo install cmd-usage / brew upgrade jeffreyjyz/tap/cmduse)"
    ))
}

/// True when dotted-numeric `latest` > `current` (missing parts count as 0).
/// Build/pre-release suffixes (`-beta.1`, `+build`) are ignored, so a
/// pre-release never wins against a release with the same numeric core.
fn newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(['-', '+'])
            .next()
            .unwrap_or(v)
            .split('.')
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
        assert!(newer("0.6.2", "0.6.1"));
        assert!(!newer("0.1.9", "0.1.10"));
        assert!(!newer("0.1.9", "0.1.9"));
        // pre-release of the same numeric core is not newer than the release
        assert!(!newer("0.6.6-beta.1", "0.6.6"));
        assert!(!newer("0.6.5+build.2", "0.6.5"));
        assert!(newer("0.6.6-beta.1", "0.6.5"));
    }

    #[test]
    fn disabled_check_skips_network() {
        assert!(check_sync(false, None).is_none());
    }

    #[test]
    fn update_message_only_when_newer() {
        assert!(update_msg(CURRENT).is_none());
        assert!(update_msg("0.0.1").is_none());
        let msg = update_msg("99.0.0").expect("newer version yields a message");
        assert!(msg.contains("update available"), "{msg}");
        assert!(msg.contains("99.0.0"), "{msg}");
    }
}
