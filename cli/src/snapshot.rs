use crate::api;
use crate::render::{Snapshot, CYAN, RESET};
use std::io::Write;

pub fn snapshot() -> Snapshot {
    let mut s = empty_snapshot();

    let key = match api::api_key() {
        Ok(k) => k,
        Err(e) => {
            s.err = Some(format!("no API key ({e}) — run `cmd login`"));
            return s;
        }
    };

    // animated spinner while the (retrying) fetches run, so a slow network
    // doesn't look hung. Written to /dev/tty directly: in watch mode the main
    // thread holds the stdout lock during redraw, and stderr may be redirected.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let writer = std::thread::spawn(move || {
        let mut tty = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/tty")
            .ok();
        let mut i = 0;
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(f) = tty.as_mut() {
                write!(
                    f,
                    "\r\x1b[2K{CYAN}fetching usage… {}{RESET}",
                    frames[i % frames.len()]
                )
                .ok();
                f.flush().ok();
            }
            i += 1;
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    });

    // three endpoints in parallel — slowest one sets the latency
    let (r_sub, r_credits, r_summary) = {
        let k1 = key.clone();
        let k2 = key.clone();
        let t1 = std::thread::spawn(move || api::subscriptions(&k1));
        let t2 = std::thread::spawn(move || api::credits(&k2));
        let t3 = std::thread::spawn(move || api::summary(&key));
        (
            t1.join()
                .unwrap_or_else(|_| Err("subscriptions thread panicked".into())),
            t2.join()
                .unwrap_or_else(|_| Err("credits thread panicked".into())),
            t3.join()
                .unwrap_or_else(|_| Err("summary thread panicked".into())),
        )
    };

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = writer.join();
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/tty") {
        // erase spinner line; main redraws status line right after
        write!(f, "\r\x1b[2K").ok();
        f.flush().ok();
    }

    let mut errs: Vec<String> = Vec::new();
    match r_sub {
        Ok(v) => {
            warn_unknown_plan(&v.plan_id);
            s.sub = v;
        }
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

/// Warn once per process when the API returns a plan id no NAME_RULE knows:
/// the dashboard would label it "Free" with no monthly cap until plans.json
/// is updated. "free"/empty are real states, not unknowns.
fn warn_unknown_plan(plan_id: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if plan_id.is_empty() || plan_id == "free" || cmduse_core::plan_rule_matched(plan_id) {
        return;
    }
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("warning: unknown plan id '{plan_id}' — showing Free; update core/plans.json");
    }
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
            window_limits: api::WindowLimits {
                five_hour: None,
                weekly: None,
            },
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
