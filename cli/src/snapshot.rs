use crate::api;
use crate::render::Snapshot;
use std::io::{IsTerminal, Write};

pub fn snapshot() -> Snapshot {
    let mut s = empty_snapshot();

    let key = match api::api_key() {
        Ok(k) => k,
        Err(e) => {
            s.err = Some(format!("no API key ({e}) — run `cmd login`"));
            return s;
        }
    };

    // one-line feedback while the (retrying) fetches run, so a slow network
    // doesn't look hung before the first/next frame. tty only, cleared below.
    let tty = std::io::stderr().is_terminal();
    if tty {
        eprint!("\r\x1b[2Kfetching usage…");
        std::io::stderr().flush().ok();
    }

    // three endpoints in parallel — slowest one sets the latency
    let (r_sub, r_credits, r_summary) = {
        let k1 = key.clone();
        let k2 = key.clone();
        let t1 = std::thread::spawn(move || api::subscriptions(&k1));
        let t2 = std::thread::spawn(move || api::credits(&k2));
        let t3 = std::thread::spawn(move || api::summary(&key));
        (
            t1.join().unwrap_or_else(|_| Err("subscriptions thread panicked".into())),
            t2.join().unwrap_or_else(|_| Err("credits thread panicked".into())),
            t3.join().unwrap_or_else(|_| Err("summary thread panicked".into())),
        )
    };
    if tty {
        eprint!("\r\x1b[2K");
        std::io::stderr().flush().ok();
    }

    let mut errs: Vec<String> = Vec::new();
    match r_sub {
        Ok(v) => s.sub = v,
        Err(e) => errs.push(e),
    }
    match r_credits {
        Ok(v) => s.credits = v,
        Err(e) => errs.push(e),
    }
    match r_summary {
        Ok(v) => s.summary = v,
        Err(e) => errs.push(e),
    }
    if !errs.is_empty() {
        s.err = Some(errs.join("; "));
    }

    s
}

fn empty_snapshot() -> Snapshot {
    let now = cmduse_core::dates::now_secs();

    Snapshot {
        sub: api::SubData {
            status: "—".into(),
            plan_id: "free".into(),
            current_period_end: None,
            current_period_start: None,
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
    }
}
