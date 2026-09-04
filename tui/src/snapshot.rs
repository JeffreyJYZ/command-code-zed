use crate::api;
use std::sync::atomic::AtomicBool;
use crate::render::Snapshot;
use std::io::Write;

const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

pub fn snapshot() -> Snapshot {
    fetch(true)
}

/// Watch-mode refresh: spinner animates while fetching, disappears when done.
pub fn snapshot_with_spinner(_spinner: bool) -> Snapshot {
    fetch(true)
}

fn fetch(_show_spinner: bool) -> Snapshot {

    let mut s = empty_snapshot();

    let key = match api::api_key() {
        Ok(k) => k,
        Err(e) => {
            s.err = Some(format!("no API key ({e}) — run `cmd login`"));
            return s;
        }
    };

    let stop = std::sync::Arc::new(AtomicBool::new(false));
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
                // /dev/tty direct — main thread holds stdout lock during redraw
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
            t1.join().unwrap_or_else(|_| Err("subscriptions thread panicked".into())),
            t2.join().unwrap_or_else(|_| Err("credits thread panicked".into())),
            t3.join().unwrap_or_else(|_| Err("summary thread panicked".into())),
        )
    };

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

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = writer.join();
    if let Some(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/tty").ok() {
        // erase spinner line; main redraws status line right after
        write!(f, "\r\x1b[2K").ok();
        f.flush().ok();
    }
    s
}

fn empty_snapshot() -> Snapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

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
