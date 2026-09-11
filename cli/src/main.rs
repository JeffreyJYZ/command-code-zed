mod api;
mod cli;
mod config;
mod paths;
mod render;
mod report_render;
mod reports;
mod snapshot;
mod update_check;

#[cfg(test)]
mod cli_tests;

#[cfg(test)]
mod config_tests;

#[cfg(test)]
mod render_tests;

#[cfg(test)]
mod main_tests;

use std::io::Write;

use crate::render::{BOLD, DIM, RESET};

fn main() {
    let args = cli::parse_args();
    if args.help {
        cli::usage();
        return;
    }

    if let Some(cs) = args.config_set {
        if let Err(e) = config::set(&cs) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // offline local reports — no API, no key needed
    match args.subcmd {
        Some(cli::SubCmd::Daily) => {
            // account-wide (all harnesses) via API when key available; local fallback
            let data_source = if args.local {
                None
            } else {
                api::api_key().ok().map(|k| reports::load_account_daily(args.last.unwrap_or(7), &k, args.tz.unwrap_or(0)))
            };
            match data_source {
                Some(Ok(by_day)) => {
                    let total = reports::sum_days(&by_day);
                    print!("{}", report_render::table("account", "all harnesses", &by_day, &total, None, args.json));
                }
                Some(Err(e)) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
                None => {
                    let d = reports::load_local();
                    print!("{}", report_render::table("local", "offline, ~/.commandcode/projects", &d.by_day, &d.total, args.last, args.json));
                }
            }
            return;
        }
        Some(cli::SubCmd::Hours) => {
            let rows = if args.local {
                reports::load_local_hourly(args.hours.unwrap_or(24))
            } else {
                let key = match api::api_key() {
                    Ok(k) => k,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    }
                };
                match reports::load_account_hourly(args.hours.unwrap_or(24), &key, args.tz.unwrap_or(0)) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    }
                }
            };
            let subtitle = if args.local {
                "local, CLI sessions".to_string()
            } else if let Some(tz) = args.tz {
                format!("all harnesses, UTC{}", cmduse_core::dates::tz_offset_suffix(tz))
            } else {
                "all harnesses, UTC".to_string()
            };
            print!("{}", report_render::hourly_table(&rows, args.json, &subtitle));
            return;
        }
        Some(cli::SubCmd::Model) => {
            let d = reports::load_local();
            print!("{}", report_render::model_table(&d.by_model, args.json));
            return;
        }
        Some(cli::SubCmd::Session) => {
            let d = reports::load_local();
            print!("{}", report_render::project_table(&d.by_project, args.json));
            return;
        }
        Some(cli::SubCmd::Statusline) => {
            statusline_cmd(&args);
            return;
        }
        Some(cli::SubCmd::Models) => {
            models_cmd(&args);
            return;
        }
        Some(cli::SubCmd::Plans) => {
            plans_cmd(&args);
            return;
        }
        None => {}
    }

    let cfg = config::load();
    let interval = args.interval.unwrap_or(cfg.interval_secs).clamp(1, 86_400);
    let bar_width = args.bar_width.unwrap_or(cfg.bar_width).max(5);
    // spend-burst sparkline is opt-in: -b/--bursts on the CLI turns it on for
    // this run; otherwise it follows the config flag (default off).
    let burst_on = args.bursts.is_some() || cfg.burst_enabled;
    let burst_cap = if burst_on {
        args.bursts.or(Some(cfg.burst_samples)).unwrap_or(40).clamp(5, 240)
    } else {
        0
    };
    let notify_on_cap = cfg.notify_on_cap;

    if args.once {
        let s = snapshot::snapshot();
        if args.json {
            println!("{}", render::render_json(&s));
        } else if args.plain {
            print!("{}", render::render_plain(&s));
        } else {
            print!("{}", render::render(&s, bar_width));
        }
        return;
    }

    // Watch mode: check update BEFORE first frame draw. check_sync() blocks
    // once per day (≤5s); an async eprintln here could land mid-redraw and
    // tear the in-place frame.
    if let Some(msg) = update_check::check_sync() {
        eprintln!("{msg}");
    }

    // live mode: true in-place redraw. Frame's last line = status line,
    // drawn WITHOUT trailing newline so the cursor stays on it. Spinner
    // and countdown rewrite that line in place. No scroll, no drift.
    //
    // Terminal size is re-queried every refresh: on a window too small for
    // the full dashboard we swap to a compact single-line frame instead of
    // exiting, so shrinking/widening mid-run follows live (term_size is NOT
    // cached).
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut prev_lines = 0usize;
    // deltas ($ per refresh) feed the spend-burst sparkline. Session-only:
    // no disk persistence — a fresh run shows a fresh trend, never old data.
    let mut history: Vec<f64> = Vec::new();
    let mut history_used: Vec<f64> = Vec::new(); // raw cumulative 5h spend
    // cap-hit notifications: fire once on the rising edge into "exceeded"
    let mut was_exceeded_5h = false;
    let mut was_exceeded_wk = false;
    loop {
        let s = snapshot::snapshot();
        let (rows, cols) = term_size().unwrap_or((0, 0));
        // <8x4 is not a usable dashboard even in compact form: keep drawing
        // the compact line anyway (clip keeps it stable), no hard exit.
        let compact = cols > 0 && (rows < 16 || cols < 40);
        // track 5-hour spend DELTA between refreshes — cumulative spend is
        // monotonic (sparkline of it is all-full); deltas show burst vs idle
        let used = s
            .credits
            .window_limits
            .five_hour
            .as_ref()
            .map(|w| w.used)
            .unwrap_or(0.0);
        let delta = history_used
            .last()
            .map(|prev| (used - prev).max(0.0))
            .unwrap_or(0.0);
        history_used.push(used);
        if history_used.len() > 60 {
            history_used.remove(0);
        }
        if burst_on {
            history.push(delta);
            if history.len() > burst_cap {
                history.remove(0);
            }
        }
        if notify_on_cap {
            let ex5 = s.credits.window_limits.five_hour.as_ref().map(|w| w.exceeded).unwrap_or(false);
            let exwk = s.credits.window_limits.weekly.as_ref().map(|w| w.exceeded).unwrap_or(false);
            if ex5 && !was_exceeded_5h {
                notify_cap("5-hour");
            }
            if exwk && !was_exceeded_wk {
                notify_cap("weekly");
            }
            was_exceeded_5h = ex5;
            was_exceeded_wk = exwk;
        }
        let text = if compact {
            compact_dashboard(&s, if burst_on { &history } else { &[] })
        } else if args.plain {
            render::render_plain(&s)
        } else {
            render::render(&s, bar_width)
        };
        let spark = if compact || !burst_on {
            String::new()
        } else if history.len() >= 2 && history.iter().any(|v| *v > 0.0) {
            format!("{DIM}spend bursts ({}s samples){RESET} {}\n", interval, render::sparkline(&history))
        } else {
            String::new() // idle (all-zero deltas) → no row, no flat-line noise
        };
        let status_line = format!(
            "{DIM}refreshing every {interval}s · ctrl-c to quit{RESET}"
        );
        let frame = format!("{text}{spark}{status_line}");
        prev_lines = redraw_frame(&mut out, &frame, prev_lines, (cols > 0).then_some(cols));
        out.flush().ok();
        // countdown: rewrite just the status line each second (cursor already on it)
        for remaining in (1..interval).rev() {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let msg = format!(
                "\r\x1b[2K{DIM}refreshing every {interval}s · next refresh in {remaining}s · ctrl-c to quit{RESET}"
            );
            write!(out, "{}", clip_to_width(&msg, (cols > 0).then_some(cols))).ok();
            out.flush().ok();
        }
    }
}

/// Fire a desktop notification that a spend window hit its cap. Best-effort:
/// runs on a detached thread and silently no-ops if the OS tool is missing.
/// macOS uses osascript, Linux notify-send.
fn notify_cap(window: &str) {
    let window = window.to_string();
    std::thread::spawn(move || {
        let title = "cmduse: limit exceeded";
        let body = format!("Command Code {window} window is over its cap");
        if cfg!(target_os = "macos") {
            let script = format!(
                "display notification \"{}\" with title \"{}\"",
                body.replace('\\', "\\\\").replace('"', "\\\""),
                title
            );
            let _ = std::process::Command::new("osascript").arg("-e").arg(script).status();
        } else if cfg!(target_os = "linux") {
            let _ = std::process::Command::new("notify-send").arg(title).arg(&body).status();
        }
    });
}

/// Minimal one-line dashboard for windows too small for the full frame:
/// plan + remaining monthly credits (clip handles the rest). Stays stable
/// where the full 14-row dashboard would scroll every refresh. When the
/// spend-burst sparkline is active it is inlined at the end of the same line
/// (clip trims it first on very narrow windows).
fn compact_dashboard(s: &crate::render::Snapshot, history: &[f64]) -> String {
    use crate::render::plan_name;
    let cap = cmduse_core::plan_monthly_cap(&s.sub.plan_id);
    let cap_txt = cap.map(cmduse_core::money).unwrap_or_else(|| "-".into());
    let rem_txt = cmduse_core::money(s.credits.credits.monthly_credits);
    let h5 = s
        .credits
        .window_limits
        .five_hour
        .as_ref()
        .map(|w| cmduse_core::money(w.used))
        .unwrap_or_else(|| "-".into());
    let status = if s.sub.status.is_empty() {
        String::new()
    } else {
        format!(" · {}", s.sub.status)
    };
    let spark = if history.len() >= 2 && history.iter().any(|v| *v > 0.0) {
        format!(" {DIM}{}{RESET}", crate::render::sparkline(history))
    } else {
        String::new()
    };
    format!("{BOLD}{}{RESET} {rem_txt}/{cap_txt} · 5h {h5}{status}{spark}\n", plan_name(&s.sub.plan_id))
}

/// Terminal (rows, cols) from `stty size`. None when not a tty or the probe
/// fails — redraw then skips clipping (safe: non-tty can't wrap).
/// Deliberately UNcached: the watch loop re-queries every refresh so resizing
/// mid-run switches between the full and compact dashboards live.
fn term_size() -> Option<(usize, usize)> {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("stty size < /dev/tty")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut it = text.split_whitespace();
    let rows = it.next()?.parse().ok()?;
    let cols = it.next()?.parse().ok()?;
    Some((rows, cols))
}

/// Keep a frame line from auto-wrapping: count visible columns ignoring SGR
/// escapes and control chars, and drop trailing visible chars that would
/// push past `cols`. Appends a reset so a truncated color run can't leak.
pub fn clip_to_width(line: &str, cols: Option<usize>) -> String {
    let Some(cols) = cols else { return line.into() };
    if cols == 0 {
        return String::new();
    }
    let mut vis = 0usize;
    let mut cs = line.chars().peekable();
    while let Some(&c) = cs.peek() {
        match c {
            '\x1b' => {
                cs.next();
                for e in cs.by_ref() {
                    if e.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            c if (c as u32) < 0x20 => {
                cs.next(); // \r etc: zero width
            }
            _ => {
                vis += 1;
                cs.next();
            }
        }
    }
    if vis <= cols {
        return line.into();
    }
    let mut out = String::new();
    let mut kept = 0usize;
    let mut cs = line.chars();
    while let Some(c) = cs.next() {
        match c {
            '\x1b' => {
                out.push(c);
                for e in cs.by_ref() {
                    out.push(e);
                    if e.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            '\r' => out.push(c), // carriage return is meaningful: keep it
            c if (c as u32) < 0x20 => {}
            _ => {
                if kept < cols {
                    out.push(c);
                    kept += 1;
                }
            }
        }
    }
    if out.ends_with("\x1b[0m") {
        out
    } else {
        out + "\x1b[0m"
    }
}

/// In-place terminal redraw of one frame. Assumes the cursor parks on the
/// previous frame's last line (no trailing newline). Returns the new frame's
/// line count so the caller can pass it back as `prev_lines`.
fn redraw_frame(
    out: &mut impl Write,
    frame: &str,
    prev_lines: usize,
    cols: Option<usize>,
) -> usize {
    if prev_lines > 0 {
        // move cursor up to top of previous frame (we're ON its last line)
        write!(out, "\x1b[{}F", prev_lines - 1).ok();
    }
    let lines: Vec<&str> = frame.lines().collect();
    let n = lines.len();
    for (i, line) in lines.iter().enumerate() {
        let line = clip_to_width(line, cols);
        if i + 1 < n {
            write!(out, "\x1b[2K\r{line}\n").ok();
        } else {
            // last line: clear and write, NO newline — cursor stays here
            write!(out, "\x1b[2K\r{line}").ok();
        }
    }
    if n < prev_lines {
        // new frame shorter than old (error frame): clear the stale rows
        // below the new frame WITHOUT touching the frame we just wrote.
        // First move down one row (no clear) so the frame's last line
        // survives, then clear-and-advance each remaining stale row,
        // clear the old bottom row, and step back up to the new bottom.
        // Writing \n at the very bottom row would scroll and desync —
        // clear in place with \x1b[2K\x1b[1B instead.
        write!(out, "\x1b[1B").ok();
        for _ in (n + 1)..prev_lines {
            write!(out, "\x1b[2K\x1b[1B").ok();
        }
        write!(out, "\x1b[2K").ok();
        write!(out, "\x1b[{}F", prev_lines - n).ok();
    }
    n
}

fn models_cmd(args: &cli::Args) {
    let key = match api::api_key() {
        Ok(k) => k,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    // --gated needs the plan + credits to know what the account may use.
    let (plan_id, unlocked) = if args.gated {
        match (api::subscriptions(&key), api::credits(&key)) {
            (Ok(s), Ok(c)) => (
                s.plan_id,
                c.credits.purchased_credits > 0.0 || c.credits.free_credits > 0.0,
            ),
            _ => (String::new(), true), // unknown → show everything
        }
    } else {
        (String::new(), true)
    };
    match api::models(&key) {
        Ok(list) => {
            if args.json {
                for m in &list {
                    if args.gated {
                        // annotate every model with the gating verdict
                        let (allowed, reason) = cmduse_core::gate(&m.id, &plan_id, unlocked);
                        let line = serde_json::json!({
                            "id": m.id,
                            "name": m.name,
                            "context_length": m.context_length,
                            "owned_by": m.owned_by,
                            "allowed": allowed,
                            "reason": reason,
                        });
                        println!("{line}");
                    } else {
                        let line = serde_json::to_string(m).unwrap_or_else(|_| "{}".into());
                        println!("{line}");
                    }
                }
                return;
            }
            let list: Vec<_> = if args.gated {
                list.into_iter()
                    .filter(|m| cmduse_core::gate_allowed(&m.id, &plan_id, unlocked))
                    .collect()
            } else {
                list
            };
            if list.is_empty() {
                println!("no models returned");
                return;
            }
            let mut rows: Vec<_> = list
                .iter()
                .map(|m| {
                    let ctx = m
                        .context_length
                        .map(|n| format!("{} ctx", render::compact(n)))
                        .unwrap_or_else(|| "-".into());
                    let name = m.name.as_deref().unwrap_or(&m.id);
                    (m.id.as_str(), name, ctx)
                })
                .collect();
            rows.sort_by(|a, b| a.1.cmp(b.1));
            for (id, name, ctx) in rows {
                println!("{name:<36} {ctx:<10} {id}");
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn plans_cmd(args: &cli::Args) {
    // current plan id is best-effort: without a key/network, show no mark.
    let current = api::api_key()
        .ok()
        .and_then(|k| api::subscriptions(&k).ok())
        .map(|s| s.plan_id)
        .unwrap_or_default();
    if args.json {
        print!("{}", render::plans_json(&current));
    } else {
        use std::io::IsTerminal;
        print!("{}", render::plans_table(&current, std::io::stdout().is_terminal()));
    }
}

fn statusline_cmd(args: &cli::Args) {
    let cfg = config::load();
    let s = snapshot::snapshot();
    let plan = render::plan_name(&s.sub.plan_id);
    let cap = render::plan_monthly_cap(&s.sub.plan_id).unwrap_or(0.0);
    if let Some(e) = &s.err {
        println!("cmduse: {e}");
        std::process::exit(1);
    }
    if args.json {
        let w = |o: &Option<api::Window>| -> (f64, f64) {
            (o.as_ref().map(|w| w.used).unwrap_or(0.0), o.as_ref().map(|w| w.cap).unwrap_or(0.0))
        };
        let (h5u, h5c) = w(&s.credits.window_limits.five_hour);
        let (wku, wkc) = w(&s.credits.window_limits.weekly);
        let out = serde_json::json!({
            "plan": plan,
            "monthlyRemaining": s.credits.credits.monthly_credits,
            "monthlyCap": cap,
            "fiveHourUsed": h5u,
            "fiveHourCap": h5c,
            "weeklyUsed": wku,
            "weeklyCap": wkc,
        });
        print!("{out}");
        return;
    }
    let h5 = s
        .credits
        .window_limits
        .five_hour
        .as_ref()
        .map(|w| (w.used, w.cap));
    let wk = s
        .credits
        .window_limits
        .weekly
        .as_ref()
        .map(|w| (w.used, w.cap));
    // pace ETAs for the {5h_eta}/{wk_eta} placeholders
    let now = s.now;
    let eta_of = |w: &Option<api::Window>, dur: u64| -> Option<String> {
        let w = w.as_ref()?;
        let secs = cmduse_core::pace_eta(w.reset_at, dur, w.used, w.cap, now)?;
        Some(format!("on pace to hit cap in {}", cmduse_core::rel_time(Some(secs * 1000.0), Some(now))))
    };
    let h5_eta = eta_of(&s.credits.window_limits.five_hour, cmduse_core::FIVE_HOUR_SECS);
    let wk_eta = eta_of(&s.credits.window_limits.weekly, cmduse_core::WEEKLY_SECS);
    let d = report_render::StatusData {
        plan,
        monthly_remaining: s.credits.credits.monthly_credits,
        monthly_cap: cap,
        five_hour: &h5,
        weekly: &wk,
        five_hour_eta: h5_eta,
        weekly_eta: wk_eta,
        bar_width: cfg.bar_width.min(30),
        colors: cfg.statusline_colors,
        ascii: cfg.statusline_ascii,
    };
    print!("{}", report_render::render_statusline(&cfg.statusline_template, &d));
}
