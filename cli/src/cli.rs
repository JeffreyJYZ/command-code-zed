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
    pub tz: Option<i64>,
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
    pub notify: Option<bool>,
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
        tz: None,
    };
    let (mut saw_once, mut saw_watch) = (false, false);
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-1" | "--once" => {
                a.once = true;
                saw_once = true;
            }
            "-p" | "--plain" => a.plain = true,
            "-W" | "--watch" | "watch" => {
                a.once = false; // explicit default mode
                saw_watch = true;
            }
            "-h" | "--help" | "help" => a.help = true,
            "-V" | "--version" | "version" => {
                println!("cmduse {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--json" => a.json = true,
            "--local" => a.local = true,
            "--gated" => a.gated = true,
            "--tz" => {
                let Some(v) = it.next() else {
                    eprintln!("--tz needs an offset like +05:30 or -08:00");
                    std::process::exit(2);
                };
                match parse_tz(&v) {
                    Some(secs) => a.tz = Some(secs),
                    None => {
                        eprintln!("--tz needs an offset like +05:30 or -08:00 (got '{v}')");
                        std::process::exit(2);
                    }
                }
            }
            "-i" | "--interval" => {
                let Some(v) = it.next() else {
                    eprintln!("-i needs a duration 1s–24h (e.g. 30, 30s, 5m, 1h)");
                    std::process::exit(2);
                };
                match parse_duration(&v) {
                    Some(n) if (1..=86_400).contains(&n) => a.interval = Some(n),
                    _ => {
                        eprintln!("-i needs a duration 1s–24h (e.g. 30, 30s, 5m, 1h) (got '{v}')");
                        std::process::exit(2);
                    }
                }
            }
            "-w" | "--bar-width" => {
                let Some(v) = it.next() else {
                    eprintln!("-w needs a number 5–200");
                    std::process::exit(2);
                };
                match v.parse::<usize>() {
                    Ok(n) if (5..=200).contains(&n) => a.bar_width = Some(n),
                    _ => {
                        eprintln!("-w needs a number 5–200 (got '{v}')");
                        std::process::exit(2);
                    }
                }
            }
            "-b" | "--bursts" => {
                let Some(v) = it.next() else {
                    eprintln!("-b needs a number 5–240");
                    std::process::exit(2);
                };
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
                                "notify" | "notify_on_cap" => match v.parse() {
                                    Ok(b) => cs.notify = Some(b),
                                    Err(_) => {
                                        eprintln!("config: notify must be true/false, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                other => {
                                    eprintln!("config: unknown key '{other}' (keys: interval, width, sl, sl_colors, sl_ascii, burst, burst_on, notify)");
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
    if saw_once && saw_watch {
        eprintln!("cannot combine -1/--once with -W/--watch");
        std::process::exit(2);
    }
    a
}

/// Parse a refresh duration: bare seconds ("30"), or with a unit suffix
/// ("30s", "5m", "1h", "2d"). Returns whole seconds.
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num, mult) = match s.chars().last()? {
        's' | 'S' => (&s[..s.len() - 1], 1u64),
        'm' | 'M' => (&s[..s.len() - 1], 60),
        'h' | 'H' => (&s[..s.len() - 1], 3600),
        'd' | 'D' => (&s[..s.len() - 1], 86400),
        _ => (s, 1),
    };
    num.parse::<u64>().ok().and_then(|n| n.checked_mul(mult))
}

/// Parse a UTC offset "+05:30" / "-08:00" / "+0530" / "-8" → seconds east.
pub fn parse_tz(s: &str) -> Option<i64> {
    let (sign, rest) = if let Some(t) = s.strip_prefix('-') {
        (-1i64, t)
    } else if let Some(t) = s.strip_prefix('+') {
        (1i64, t)
    } else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
    let (h, m) = match rest.split_once(':') {
        Some((h, m)) => (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?),
        None if rest.len() == 4 => {
            let (h, m) = rest.split_at(2);
            (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?)
        }
        None => (rest.parse::<i64>().ok()?, 0),
    };
    if h > 14 || m > 59 {
        return None;
    }
    Some(sign * (h * 3600 + m * 60))
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
  -W, --watch           Force watch mode (default; conflicts with -1)
  -p, --plain           No colors / no live redraw (for scripts, pipes)
  -i, --interval <dur>  Refresh interval (default: config or 5s). Accepts a
                        unit suffix: 30s, 5m, 1h (bare number = seconds)
  -w, --bar-width <n>   Progress bar width in chars (default: config or 20)
  -b, --bursts <n>      Spend-burst sparkline samples (default: 40; hide when
                        idle via config burst_on=false)
      --json            Machine-readable JSON output where supported
      --gated           models: filter to what the current plan allows
      --tz <±HH:MM>     daily/hourly: bucket by this UTC offset instead of UTC
      --days <n>        daily: number of days back (default 7, max 365)
      --hours <n>       hourly: number of hours back (default 24, max 168)
      --local           daily: use local CLI logs only (skip account API)
  -V, --version         Print version
  -h, --help            This help

Config: ~/.config/cmd-usage/config.json
  { \"interval_secs\": 5, \"bar_width\": 20, \"burst_samples\": 40 }

daily/model/session read ~/.commandcode/projects offline (no API calls).
Dashboard needs: logged-in Command Code CLI (~/.commandcode/auth.json)"
}
