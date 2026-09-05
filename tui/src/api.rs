use serde::Deserialize;

const API_BASE: &str = "https://api.commandcode.ai";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credits {
    pub monthly_credits: f64,
    #[serde(default)]
    pub purchased_credits: f64,
    #[serde(default)]
    pub free_credits: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub used: f64,
    pub cap: f64,
    #[serde(default)]
    pub exceeded: bool,
    #[serde(default)]
    pub reset_at: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WindowLimits {
    #[serde(default)]
    pub five_hour: Option<Window>,
    #[serde(default)]
    pub weekly: Option<Window>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditsResp {
    pub credits: Credits,
    #[serde(default)]
    pub window_limits: WindowLimits,
}

#[derive(Deserialize)]
struct SubscriptionsResp {
    #[serde(default)]
    data: Option<SubData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubData {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub plan_id: String,
    #[serde(default)]
    pub current_period_end: Option<String>,
    #[serde(default)]
    pub current_period_start: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    #[serde(default)]
    pub total_count: u64,
    #[serde(default)]
    pub total_cost: f64,
    #[serde(default)]
    pub success_rate: f64,
    #[serde(default)]
    pub total_tokens_in: u64,
    #[serde(default)]
    pub total_tokens_out: u64,
}

pub fn api_key() -> std::io::Result<String> {
    // CMD_API_KEY override: use any account key without touching auth.json
    // (multi-account / testing)
    if let Ok(k) = std::env::var("CMD_API_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    let path = home().join(".commandcode/auth.json");
    let text = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    v["apiKey"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no apiKey"))
}

fn home() -> std::path::PathBuf {
    std::env::var("HOME").unwrap_or_else(|_| "/".into()).into()
}

fn get(path: &str, key: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(&format!("{API_BASE}{path}"))
        .set("Authorization", &format!("Bearer {key}"))
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| format!("{path}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(10 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{path}: {e}"))?;
    Ok(buf)
}

use std::io::Read;

pub fn subscriptions(key: &str) -> Result<SubData, String> {
    let d: SubscriptionsResp = serde_json::from_slice(&get("/alpha/billing/subscriptions", key)?)
        .map_err(|e| format!("subscriptions: {e}"))?;
    Ok(d.data.unwrap_or(SubData {
        status: "none".into(),
        plan_id: "free".into(),
        current_period_end: None,
            current_period_start: None,
    }))
}

pub fn credits(key: &str) -> Result<CreditsResp, String> {
    serde_json::from_slice(&get("/alpha/billing/credits", key)?)
        .map_err(|e| format!("credits: {e}"))
}

pub fn summary(key: &str) -> Result<UsageSummary, String> {
    serde_json::from_slice(&get("/alpha/usage/summary", key)?)
        .map_err(|e| format!("summary: {e}"))
}
