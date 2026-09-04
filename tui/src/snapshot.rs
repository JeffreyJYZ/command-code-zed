use crate::api;
use crate::render::Snapshot;

pub fn snapshot() -> Snapshot {
    snapshot_with_spinner(true)
}

/// `spinner=false` silences the fetch animation (used for silent auto-refreshes).
pub fn snapshot_with_spinner(spinner: bool) -> Snapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut s = Snapshot {
        sub: api::SubData {
            status: "—".into(),
            plan_id: "free".into(),
            current_period_end: None,
        },
        credits: api::CreditsResp {
            credits: api::Credits {
                monthly_credits: 0.0,
                purchased_credits: 0.0,
                free_credits: 0.0,
            },
            window_limits: api::WindowLimits { five_hour: None, weekly: None },
        },
        summary: api::UsageSummary {
            total_count: 0,
            total_cost: 0.0,
            success_rate: 0.0,
            total_tokens_in: 0,
            total_tokens_out: 0,
        },
        now,
        err: None,
    };

    let key = match api::api_key() {
        Ok(k) => k,
        Err(e) => {
            s.err = Some(format!("no API key ({e}) — run `cmd login`"));
            return s;
        }
    };

    if spinner {
        crate::spin::Spinner::global().start("fetching usage…");
    }
    match (|| -> Result<(), String> {
        s.sub = api::subscriptions(&key)?;
        s.credits = api::credits(&key)?;
        s.summary = api::summary(&key)?;
        Ok(())
    })() {
        Ok(()) => {}
        Err(e) => s.err = Some(e),
    }
    if spinner {
        crate::spin::Spinner::global().stop();
    }
    s
}
