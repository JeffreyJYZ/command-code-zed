mod api;
mod cli;
mod config;
mod render;
mod snapshot;
mod spin;

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

    // live mode: true in-place redraw — cursor to first line of frame,
    // rewrite with per-line clear. First frame: print normally.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut prev_lines = 0usize;
    let mut first = true;
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut tick = 0usize;
    loop {
        // spinner on first fetch only; auto-refreshes redraw frame itself
        let s = snapshot::snapshot_with_spinner(first);
        let text = if args.plain {
            render::render_plain(&s, bar_width)
        } else {
            render::render(&s, bar_width)
        };
        tick += 1;
        let status_line = format!(
            "{DIM}refreshing every {interval}s · updated {} {}· ctrl-c to quit{RESET}",
            timestamp(),
            frames[tick % frames.len()],
        );
        let frame = format!("{text}{status_line}\n");
        if !first {
            // move cursor up to top of previous frame
            write!(out, "\x1b[{prev_lines}F").ok();
        }
        for line in frame.lines() {
            // clear line, move to col 0, write, newline
            write!(out, "\x1b[2K\r{line}\n").ok();
        }
        out.flush().ok();
        prev_lines = frame.lines().count();
        first = false;
        std::thread::sleep(std::time::Duration::from_secs(interval));
    }
}

fn timestamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}s", d.as_secs() % 86400) // rough time-of-day in secs; good enough as tick marker
}
