#[derive(Default)]
pub struct Args {
    pub interval: Option<u64>,
    pub once: bool,
    pub plain: bool,
    pub bar_width: Option<usize>,
    pub bursts: Option<usize>,
    pub help: bool,
    pub config_set: Option<ConfigSet>,
    pub subcmd: Option<SubCmd>,
    pub last: Option<usize>,
    pub hours: Option<usize>,
    pub json: bool,
    pub local: bool,
    pub gated: bool,
}

#[derive(Debug, Default)]
pub struct ConfigSet {
    pub interval: Option<u64>,
    pub width: Option<usize>,
    pub sl_template: Option<String>,
    pub sl_colors: Option<bool>,
    pub sl_ascii: Option<bool>,
    pub bursts: Option<usize>,
    pub burst_on: Option<bool>,
}

#[derive(Debug, PartialEq)]
pub enum SubCmd {
    Daily,
    Hours,
    Model,
    Session,
    Statusline,
    Models,
    Plans,
}

pub fn parse_args() -> Args {
    let mut a = Args {
        interval: None,
        once: false,
        plain: false,
        bar_width: None,
        bursts: None,
        help: false,
        config_set: None,
        subcmd: None,
        last: None,
        hours: None,
        json: false,
        local: false,
        gated: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-1" | "--once" => a.once = true,
            "-p" | "--plain" => a.plain = true,
            "-W" | "--watch" => a.once = false, // explicit default mode
            "-h" | "--help" | "help" => a.help = true,
            "-V" | "--version" | "version" => {
                println!("cmduse {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--json" => a.json = true,
            "--local" => a.local = true,
            "--gated" => a.gated = true,
            "-i" | "--interval" => {
                let v = it.next().expect("interval needs a value");
                match v.parse::<u64>() {
                    Ok(n) if (1..=86_400).contains(&n) => a.interval = Some(n),
                    _ => {
                        eprintln!("-i needs a number 1–86400 (got '{v}')");
                        std::process::exit(2);
                    }
                }
            }
            "-w" | "--bar-width" => {
                let v = it.next().expect("bar-width needs a value");
                match v.parse::<usize>() {
                    Ok(n) if (5..=200).contains(&n) => a.bar_width = Some(n),
                    _ => {
                        eprintln!("-w needs a number 5–200 (got '{v}')");
                        std::process::exit(2);
                    }
                }
            }
            "-b" | "--bursts" => {
                let v = it.next().expect("bursts needs a value");
                match v.parse::<usize>() {
                    Ok(n) if (5..=240).contains(&n) => a.bursts = Some(n),
                    _ => {
                        eprintln!("-b needs a number 5–240 (got '{v}')");
                        std::process::exit(2);
                    }
                }
            }
            "--days" | "--last" | "-l" => match it.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(n) => a.last = Some(n.clamp(1, 365)),
                None => {
                    eprintln!("--days needs a number (e.g. --days 7)");
                    std::process::exit(2);
                }
            },
            "--hours" => match it.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(n) => a.hours = Some(n.clamp(1, 168)),
                None => {
                    eprintln!("--hours needs a number (e.g. --hours 6, max 168)");
                    std::process::exit(2);
                }
            },
            "daily" | "days" => a.subcmd = Some(SubCmd::Daily),
            "hourly" | "hours" => a.subcmd = Some(SubCmd::Hours),
            "model" => a.subcmd = Some(SubCmd::Model),
            "session" | "sessions" | "project" => a.subcmd = Some(SubCmd::Session),
            "models" => a.subcmd = Some(SubCmd::Models),
            "plans" => a.subcmd = Some(SubCmd::Plans),
            "statusline" => a.subcmd = Some(SubCmd::Statusline),
            "config" => {
                // config set [interval=<s>] [width=<n>]
                if let Some(sub) = it.next() {
                    if sub == "set" {
                        let mut cs = ConfigSet::default();
                        for kv in it.by_ref() {
                            let (k, v) = match kv.split_once('=') {
                                Some(kv) => kv,
                                None => {
                                    eprintln!("config: expected key=value, got '{kv}' (keys: interval, width)");
                                    std::process::exit(2);
                                }
                            };
                            match k {
                                "interval" => match v.parse() {
                                    Ok(n) => cs.interval = Some(n),
                                    Err(_) => {
                                        eprintln!("config: interval must be a number, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                "width" => match v.parse() {
                                    Ok(n) => cs.width = Some(n),
                                    Err(_) => {
                                        eprintln!("config: width must be a number, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                "sl" | "statusline" => cs.sl_template = Some(v.to_string()),
                                "sl_colors" => match v.parse() {
                                    Ok(b) => cs.sl_colors = Some(b),
                                    Err(_) => {
                                        eprintln!("config: sl_colors must be true/false, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                "sl_ascii" => match v.parse() {
                                    Ok(b) => cs.sl_ascii = Some(b),
                                    Err(_) => {
                                        eprintln!("config: sl_ascii must be true/false, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                "burst" | "bursts" => match v.parse() {
                                    Ok(n) => cs.bursts = Some(n),
                                    Err(_) => {
                                        eprintln!("config: bursts must be a number, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                "burst_on" | "burst-on" => match v.parse() {
                                    Ok(b) => cs.burst_on = Some(b),
                                    Err(_) => {
                                        eprintln!("config: burst_on must be true/false, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                other => {
                                    eprintln!("config: unknown key '{other}' (keys: interval, width, sl, sl_colors, sl_ascii, burst, burst_on)");
                                    std::process::exit(2);
                                }
                            }
                        }
                        a.config_set = Some(cs);
                    } else {
                        eprintln!("config: unknown subcommand '{sub}' (try: config set interval=10)");
                        std::process::exit(2);
                    }
                } else {
                    eprintln!("config: missing subcommand (try: config set interval=10)");
                    std::process::exit(2);
                }
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
    println!("{}", usage_text());
}

pub fn usage_text() -> &'static str {
    "cmduse — Command Code usage dashboard

Usage: cmduse [options]           Live plan dashboard (watch mode)
       cmduse -1                  One-shot dashboard
       cmduse daily [--days N] [--json]    Account usage by day (all harnesses)
       cmduse hourly [--hours N] [--json]  Account usage by hour (all harnesses)
       cmduse model [--json]      Local usage by model
       cmduse session [--json]    Local usage by project/session
       cmduse models              Live model list from the Command Code API
       cmduse plans               Plan comparison table
       cmduse statusline          Compact one-liner for prompts/tmux
       cmduse config set interval=<s> width=<n> burst_on=true burst=40

Options:
  -1, --once            Fetch once, print, exit (no watch)
  -W, --watch           Force watch mode (default; overrides an earlier -1)
  -p, --plain           No colors / no live redraw (for scripts, pipes)
  -i, --interval <s>    Refresh interval in seconds (default: config or 5)
  -w, --bar-width <n>   Progress bar width in chars (default: config or 20)
  -b, --bursts <n>      Spend-burst sparkline samples (default: 40; hide when
                        idle via config burst_on=false)
      --json            Machine-readable JSON output where supported
      --gated           models: filter to what the current plan allows
      --days <n>        daily: number of days back (default 7, max 365)
      --hours <n>       hourly: number of hours back (default 24, max 168)
      --json            Machine-readable JSON output
      --local           daily: use local CLI logs only (skip account API)
  -V, --version         Print version
  -h, --help            This help

Config: ~/.config/cmd-usage/config.json
  { \"interval_secs\": 5, \"bar_width\": 20, \"burst_samples\": 40 }

daily/model/session read ~/.commandcode/projects offline (no API calls).
Dashboard needs: logged-in Command Code CLI (~/.commandcode/auth.json)"
}
