use cmduse_core::SubscriptionsResp;
pub use cmduse_core::{Credits, CreditsResp, SubData, UsageSummary, Window, WindowLimits};
use serde::{Deserialize, Serialize};

const API_BASE: &str = "https://api.commandcode.ai";

pub fn api_key() -> std::io::Result<String> {
    // CMD_API_KEY override: use any account key without touching auth.json
    // (multi-account / testing)
    if let Ok(k) = std::env::var("CMD_API_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    let path = crate::paths::home().join(".commandcode/auth.json");
    let text = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    v["apiKey"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no apiKey"))
}

fn get(path: &str, key: &str) -> Result<Vec<u8>, String> {
    const MAX_RETRIES: u32 = 5;
    const BASE_DELAY_MS: u64 = 1000;
    const MAX_DELAY_MS: u64 = 8000;

    let mut last_err = String::new();
    for attempt in 0..=MAX_RETRIES {
        let resp = ureq::get(&format!("{API_BASE}{path}"))
            .set("Authorization", &format!("Bearer {key}"))
            .timeout(std::time::Duration::from_secs(15))
            .call();

        let mut retry_after: Option<u64> = None;
        match resp {
            Ok(resp) => {
                const MAX_BODY: u64 = 10 * 1024 * 1024;
                let mut buf = Vec::new();
                match resp.into_reader().take(MAX_BODY).read_to_end(&mut buf) {
                    // hitting the cap means the body was truncated and will
                    // never parse — fail loudly instead of retrying a huge body
                    Ok(n) if n as u64 == MAX_BODY => {
                        last_err =
                            format!("{path}: response exceeds {} MiB", MAX_BODY / 1024 / 1024);
                        break;
                    }
                    Ok(_) => return Ok(buf),
                    Err(e) => last_err = format!("{path}: read error: {e}"),
                }
            }
            Err(e) => {
                last_err = format!("{path}: {e}");
                // Transport = network/TLS/socket blip (always retry); Status =
                // 429/5xx retry, other 4xx client errors are fatal.
                let is_transient = match &e {
                    ureq::Error::Status(code, resp) => {
                        // honor a numeric Retry-After (seconds); capped so a
                        // hostile/huge value can't stall the watch loop for long.
                        // ponytail: HTTP-date form ignored — servers use seconds
                        // for 429. upgrade: parse the date form if one shows up.
                        if *code == 429 {
                            retry_after = resp
                                .header("Retry-After")
                                .and_then(|v| v.trim().parse::<u64>().ok())
                                .map(|s| s.min(60));
                        }
                        *code == 429 || (500..600).contains(code)
                    }
                    ureq::Error::Transport(_) => true,
                };
                if !is_transient || attempt == MAX_RETRIES {
                    break;
                }
            }
        }

        if attempt < MAX_RETRIES {
            let backoff = std::cmp::min(BASE_DELAY_MS * (1u64 << attempt), MAX_DELAY_MS);
            let delay = retry_after.map_or(backoff, |s| s * 1000);
            // jitter de-syncs retries from other clients; subsec nanos is plenty
            let jitter = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| u64::from(d.subsec_nanos()) % 500)
                .unwrap_or(0);
            std::thread::sleep(std::time::Duration::from_millis(delay + jitter));
        }
    }
    Err(last_err)
}

use std::io::Read;

pub fn subscriptions(key: &str) -> Result<SubData, String> {
    let d: SubscriptionsResp = serde_json::from_slice(&get("/alpha/billing/subscriptions", key)?)
        .map_err(|e| format!("subscriptions: {e}"))?;
    Ok(d.data_or_free())
}

pub fn credits(key: &str) -> Result<CreditsResp, String> {
    serde_json::from_slice(&get("/alpha/billing/credits", key)?)
        .map_err(|e| format!("credits: {e}"))
}

pub fn summary(key: &str) -> Result<UsageSummary, String> {
    serde_json::from_slice(&get("/alpha/usage/summary", key)?).map_err(|e| format!("summary: {e}"))
}

/// Fetch `/alpha/usage/summary?since=<ISO>` and return parsed summary.
/// Retries on transient errors (uses same logic as `get`).
pub fn summary_since(since: &str, key: &str) -> Result<UsageSummary, String> {
    serde_json::from_slice(&get(&format!("/alpha/usage/summary?since={since}"), key)?)
        .map_err(|e| format!("summary since={since}: {e}"))
}

#[derive(Deserialize, Serialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub owned_by: Option<String>,
}

#[derive(Deserialize)]
struct ModelsResp {
    #[serde(default)]
    data: Vec<ModelInfo>,
}

/// Live model list from the provider endpoint (/provider/v1/models) — the
/// same list the opencode plugin gates per plan. Raw here, no plan filter.
pub fn models(key: &str) -> Result<Vec<ModelInfo>, String> {
    let r: ModelsResp =
        serde_json::from_slice(&get("/provider/v1/models", key)?).map_err(|e| e.to_string())?;
    Ok(r.data)
}
