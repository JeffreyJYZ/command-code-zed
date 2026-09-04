mod api;
mod cli;
mod config;
mod render;
mod snapshot;

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

    let cfg = config::load();
    let interval = args.interval.or(Some(cfg.interval_secs)).unwrap_or(5).max(1);
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
