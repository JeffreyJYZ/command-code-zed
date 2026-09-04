mod api;
mod config;
mod render;

use std::io::Write;

const RESET: &str = "\x1b[0m";
const CLEAR_BELOW: &str = "\x1b[0J";
const DIM: &str = "\x1b[2m";

struct Args {
    interval: Option<u64>,
    once: bool,
    plain: bool,
    bar_width: Option<usize>,
    help: bool,
}

fn parse_args() -> Args {
    let mut a = Args { interval: None, once: false, plain: false, bar_width: None, help: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-1" | "--once" => a.once = true,
            "-p" | "--plain" => a.plain = true,
            "-h" | "--help" => a.help = true,
            "-i" | "--interval" => {
                a.interval = it.next().and_then(|v| v.parse().ok());
            }
            "-w" | "--bar-width" => {
                a.bar_width = it.next().and_then(|v| v.parse().ok());
            }
            other => {
                eprintln!("unknown arg: {other} (try --help)");
                std::process::exit(2);
            }
        }
    }
    a
}

fn usage() {
    println!(
        "cmduse — Command Code usage dashboard

Usage: cmduse [options]

Options:
  -1, --once            Fetch once, print, exit (no watch)
  -p, --plain           No colors / no live redraw (for scripts, pipes)
  -i, --interval <s>    Refresh interval in seconds (default: config or 5)
  -w, --bar-width <n>   Progress bar width in chars (default: config or 20)
  -h, --help            This help

Config: ~/.config/cmd-usage/config.json
  {{ \"interval_secs\": 5, \"bar_width\": 20 }}

Requires: logged-in Command Code CLI (~/.commandcode/auth.json)"
    );
}

fn snapshot() -> render::Snapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut s = render::Snapshot {
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

    match (|| -> Result<(), String> {
        s.sub = api::subscriptions(&key)?;
        s.credits = api::credits(&key)?;
        s.summary = api::summary(&key)?;
        Ok(())
    })() {
        Ok(()) => {}
        Err(e) => s.err = Some(e),
    }
    s
}

fn main() {
    let args = parse_args();
    if args.help {
        usage();
        return;
    }

    let cfg = config::load();
    let interval = args.interval.or(Some(cfg.interval_secs)).unwrap_or(5).max(1);
    let bar_width = args.bar_width.or(Some(cfg.bar_width)).unwrap_or(20).max(5);

    if args.once {
        let s = snapshot();
        if args.plain {
            print!("{}", render::render_plain(&s, bar_width));
        } else {
            print!("{}", render::render(&s, bar_width));
        }
        return;
    }

    // live mode: redraw in place
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    loop {
        let s = snapshot();
        let text = if args.plain {
            render::render_plain(&s, bar_width)
        } else {
            render::render(&s, bar_width)
        };
        write!(out, "\r{CLEAR_BELOW}{text}\n{DIM}refreshing every {interval}s · ctrl-c to quit{RESET}").ok();
        out.flush().ok();
        std::thread::sleep(std::time::Duration::from_secs(interval));
    }
}
