use crate::api;
use std::sync::atomic::AtomicBool;
use crate::render::Snapshot;
use std::io::{IsTerminal, Write};

const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

pub fn snapshot() -> Snapshot {
    fetch(true)
}

/// Watch-mode refresh: spinner animates while fetching, disappears when done.
pub fn snapshot_with_spinner(_spinner: bool) -> Snapshot {
    fetch(true)
}

fn fetch(show_spinner: bool) -> Snapshot {
    let is_tty = show_spinner && std::io::stdout().is_terminal();

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
        let mut i = 0;
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            if is_tty {
                let mut out = std::io::stdout();
                write!(
                    out,
                    "\r\x1b[2K{CYAN}fetching usage… {}{RESET}",
                    frames[i % frames.len()]
                )
                .ok();
                out.flush().ok();
            }
            i += 1;
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    });

    match (|| -> Result<(), String> {
        s.sub = api::subscriptions(&key)?;
        s.credits = api::credits(&key)?;
        s.summary = api::summary(&key)?;
        Ok(())
    })() {
        Ok(()) => {}
        Err(e) => s.err = Some(e),
    }

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = writer.join();
    if is_tty {
        // erase spinner line; watch mode redraws frame below cursor position
        let mut out = std::io::stdout();
        write!(out, "\r\x1b[2K").ok();
        out.flush().ok();
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
