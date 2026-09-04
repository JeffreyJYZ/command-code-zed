mod api;
mod cli;
mod config;
mod render;
mod report_render;
mod reports;
mod snapshot;

#[cfg(test)]
mod cli_tests;

#[cfg(test)]
mod config_tests;

#[cfg(test)]
mod render_tests;

use std::io::Write;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

fn main() {
    let args = cli::parse_args();
    if args.help {
        cli::usage();
        return;
    }

    if let Some((interval, width)) = args.config_set {
        if let Err(e) = config::set(interval, width) {
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
                api::api_key().ok().map(|k| reports::load_account_daily(args.last.unwrap_or(7), &k))
            };
            match data_source {
                Some(Ok(by_day)) => {
                    let total = reports::sum_days(&by_day);
                    print!("{}", report_render::account_table(&by_day, &total, args.json));
                }
                Some(Err(e)) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
                None => {
                    let d = reports::load_local();
                    print!("{}", report_render::table(&d.by_day, &d.total, args.last, args.json));
                }
            }
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
        None => {}
    }

    let cfg = config::load();
    let interval = args
        .interval
        .or(Some(cfg.interval_secs))
        .unwrap_or(5)
        .clamp(1, 86_400);
    let bar_width = args.bar_width.or(Some(cfg.bar_width)).unwrap_or(20).max(5);

    if args.once {
        let s = snapshot::snapshot();
        if args.plain {
            print!("{}", render::render_plain(&s, bar_width));
        } else {
            print!("{}", render::render(&s, bar_width));
        }
        return;
    }

    // live mode: true in-place redraw. Frame's last line = status line,
    // drawn WITHOUT trailing newline so the cursor stays on it. Spinner
    // and countdown rewrite that line in place. No scroll, no drift.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut prev_lines = 0usize;
    loop {
        let s = snapshot::snapshot_with_spinner(prev_lines > 0);
        let text = if args.plain {
            render::render_plain(&s, bar_width)
        } else {
            render::render(&s, bar_width)
        };
        let status_line = format!(
            "{DIM}refreshing every {interval}s · ctrl-c to quit{RESET}"
        );
        if prev_lines > 0 {
            // move cursor up to top of previous frame (we're ON its last line)
            write!(out, "\x1b[{}F", prev_lines - 1).ok();
        }
        let frame = format!("{text}{status_line}");
        let lines: Vec<&str> = frame.lines().collect();
        let n = lines.len();
        for (i, line) in lines.iter().enumerate() {
            if i + 1 < n {
                write!(out, "\x1b[2K\r{line}\n").ok();
            } else {
                // last line: clear and write, NO newline — cursor stays here
                write!(out, "\x1b[2K\r{line}").ok();
            }
        }
        out.flush().ok();
        prev_lines = n;
        // countdown: rewrite just the status line each second (cursor already on it)
        for remaining in (1..interval).rev() {
            std::thread::sleep(std::time::Duration::from_secs(1));
            write!(
                out,
                "\r\x1b[2K{DIM}refreshing every {interval}s · next refresh in {remaining}s · ctrl-c to quit{RESET}"
            ).ok();
            out.flush().ok();
        }
    }
}

fn statusline_cmd(args: &cli::Args) {
    let _cfg = config::load();
    let s = snapshot::snapshot_with_spinner(true);
    let plan = render::plan_name(&s.sub.plan_id);
    let cap = render::plan_monthly_cap(&s.sub.plan_id).unwrap_or(0.0);
    if let Some(e) = &s.err {
        println!("cmduse: {e}");
        std::process::exit(1);
    }
    if args.json {
        print!(
            "{{\"plan\":\"{plan}\",\"monthlyRemaining\":{:.2},\"monthlyCap\":{:.2},\"fiveHourUsed\":{:.2},\"fiveHourCap\":{:.2},\"weeklyUsed\":{:.2},\"weeklyCap\":{:.2}}}",
            s.credits.credits.monthly_credits,
            cap,
            s.credits.window_limits.five_hour.as_ref().map(|w| w.used).unwrap_or(0.0),
            s.credits.window_limits.five_hour.as_ref().map(|w| w.cap).unwrap_or(0.0),
            s.credits.window_limits.weekly.as_ref().map(|w| w.used).unwrap_or(0.0),
            s.credits.window_limits.weekly.as_ref().map(|w| w.cap).unwrap_or(0.0),
        );
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
    print!("{}", report_render::statusline(plan, s.credits.credits.monthly_credits, cap, h5, wk));
}
