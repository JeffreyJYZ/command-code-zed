pub struct Args {
    pub interval: Option<u64>,
    pub once: bool,
    pub plain: bool,
    pub bar_width: Option<usize>,
    pub help: bool,
}

pub fn parse_args() -> Args {
    let mut a = Args { interval: None, once: false, plain: false, bar_width: None, help: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-1" | "--once" => a.once = true,
            "-p" | "--plain" => a.plain = true,
            "-h" | "--help" | "help" => a.help = true,
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

pub fn usage() {
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
