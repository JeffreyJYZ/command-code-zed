//! Command Code API wire shapes shared by the CLI and the Zed extension.
//! Field names map to the JSON via camelCase rename.
use serde::Deserialize;

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
pub struct SubscriptionsResp {
    #[serde(default)]
    pub data: Option<SubData>,
}

impl SubscriptionsResp {
    /// The subscription data, or a "none/free" placeholder when the API
    /// returns no `data` block.
    pub fn data_or_free(self) -> SubData {
        self.data.unwrap_or(SubData {
            status: "none".into(),
            plan_id: "free".into(),
            current_period_end: None,
            current_period_start: None,
        })
    }
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
    // Option so a partial/unavailable summary renders as "—" rather than a
    // fake 0. Callers that aggregate treat None as 0.
    #[serde(default)]
    pub total_count: Option<u64>,
    #[serde(default)]
    pub total_cost: Option<f64>,
    #[serde(default)]
    pub success_rate: Option<f64>,
    #[serde(default)]
    pub total_tokens_in: Option<u64>,
    #[serde(default)]
    pub total_tokens_out: Option<u64>,
}
