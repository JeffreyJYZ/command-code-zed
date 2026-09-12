use lexopt::prelude::*;

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
    match parse_args_from(std::env::args_os()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}

/// Parse a full argv (binary name first, as `std::env::args_os` yields).
/// Split out so tests can drive it without touching the real process args.
pub(crate) fn parse_args_from(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<Args, String> {
    let mut a = Args::default();
    let mut parser = lexopt::Parser::from_iter(args);
    let (mut saw_once, mut saw_watch) = (false, false);
    while let Some(arg) = parser.next().map_err(|e| e.to_string())? {
        match arg {
            Short('1') | Long("once") => {
                a.once = true;
                saw_once = true;
            }
            Short('p') | Long("plain") => a.plain = true,
            Short('W') | Long("watch") => {
                a.once = false; // explicit default mode
                saw_watch = true;
            }
            Short('h') | Long("help") => a.help = true,
            Short('V') | Long("version") => {
                println!("cmduse {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            Long("json") => a.json = true,
            Long("local") => a.local = true,
            Long("gated") => a.gated = true,
            Long("tz") => {
                let v = need_value(&mut parser, "--tz needs an offset like +05:30 or -08:00")?;
                match parse_tz(&v) {
                    Some(secs) => a.tz = Some(secs),
                    None => {
                        return Err(format!(
                            "--tz needs an offset like +05:30 or -08:00 (got '{v}')"
                        ))
                    }
                }
            }
            Short('i') | Long("interval") => {
                let v = need_value(
                    &mut parser,
                    "-i needs a duration 1s–24h (e.g. 30, 30s, 5m, 1h)",
                )?;
                match parse_duration(&v) {
                    Some(n) if (1..=86_400).contains(&n) => a.interval = Some(n),
                    _ => {
                        return Err(format!(
                            "-i needs a duration 1s–24h (e.g. 30, 30s, 5m, 1h) (got '{v}')"
                        ))
                    }
                }
            }
            Short('w') | Long("bar-width") => {
                let v = need_value(&mut parser, "-w needs a number 5–200")?;
                match v.parse::<usize>() {
                    Ok(n) if (5..=200).contains(&n) => a.bar_width = Some(n),
                    _ => return Err(format!("-w needs a number 5–200 (got '{v}')")),
                }
            }
            Short('b') | Long("bursts") => {
                let v = need_value(&mut parser, "-b needs a number 5–240")?;
                match v.parse::<usize>() {
                    Ok(n) if (5..=240).contains(&n) => a.bursts = Some(n),
                    _ => return Err(format!("-b needs a number 5–240 (got '{v}')")),
                }
            }
            Long("days") | Long("last") | Short('l') => {
                let v = need_value(&mut parser, "--days needs a number (e.g. --days 7)")?;
                match v.parse::<usize>() {
                    Ok(n) => a.last = Some(n.clamp(1, 365)),
                    Err(_) => return Err("--days needs a number (e.g. --days 7)".into()),
                }
            }
            Long("hours") => {
                let v = need_value(
                    &mut parser,
                    "--hours needs a number (e.g. --hours 6, max 168)",
                )?;
                match v.parse::<usize>() {
                    Ok(n) => a.hours = Some(n.clamp(1, 168)),
                    Err(_) => return Err("--hours needs a number (e.g. --hours 6, max 168)".into()),
                }
            }
            Value(v) => match v.to_string_lossy().as_ref() {
                "watch" => {
                    a.once = false;
                    saw_watch = true;
                }
                "help" => a.help = true,
                "version" => {
                    println!("cmduse {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "daily" | "days" => a.subcmd = Some(SubCmd::Daily),
                "hourly" | "hours" => a.subcmd = Some(SubCmd::Hours),
                "model" => a.subcmd = Some(SubCmd::Model),
                "session" | "sessions" | "project" => a.subcmd = Some(SubCmd::Session),
                "models" => a.subcmd = Some(SubCmd::Models),
                "plans" => a.subcmd = Some(SubCmd::Plans),
                "statusline" => a.subcmd = Some(SubCmd::Statusline),
                "config" => a.config_set = Some(parse_config_set(&mut parser)?),
                other => return Err(format!("unknown arg: {other} (try --help)")),
            },
            Short(c) => return Err(format!("unknown arg: -{c} (try --help)")),
            Long(l) => return Err(format!("unknown arg: --{l} (try --help)")),
        }
    }
    if saw_once && saw_watch {
        return Err("cannot combine -1/--once with -W/--watch".into());
    }
    Ok(a)
}

fn need_value(parser: &mut lexopt::Parser, msg: &str) -> Result<String, String> {
    parser
        .value()
        .map(|v| v.to_string_lossy().into_owned())
        .map_err(|_| msg.to_string())
}

/// `config set [key=value …]` — the rest of argv after the `config` positional.
fn parse_config_set(parser: &mut lexopt::Parser) -> Result<ConfigSet, String> {
    match parser.next().map_err(|e| e.to_string())? {
        Some(Value(v)) if v.to_string_lossy() == "set" => {}
        Some(Value(v)) => {
            return Err(format!(
                "config: unknown subcommand '{}' (try: config set interval=10)",
                v.to_string_lossy()
            ))
        }
        _ => return Err("config: missing subcommand (try: config set interval=10)".into()),
    }
    let mut cs = ConfigSet::default();
    while let Some(arg) = parser.next().map_err(|e| e.to_string())? {
        let Value(kv) = arg else {
            return Err("config: expected key=value".into());
        };
        let kv = kv.to_string_lossy();
        let Some((k, v)) = kv.split_once('=') else {
            return Err(format!(
                "config: expected key=value, got '{kv}' (keys: interval, width)"
            ));
        };
        match k {
            "interval" => {
                cs.interval = Some(
                    v.parse()
                        .map_err(|_| format!("config: interval must be a number, got '{v}'"))?,
                )
            }
            "width" => {
                cs.width = Some(
                    v.parse()
                        .map_err(|_| format!("config: width must be a number, got '{v}'"))?,
                )
            }
            "sl" | "statusline" => cs.sl_template = Some(v.to_string()),
            "sl_colors" => cs.sl_colors = Some(parse_bool(v, "sl_colors")?),
            "sl_ascii" => cs.sl_ascii = Some(parse_bool(v, "sl_ascii")?),
            "burst" | "bursts" => {
                cs.bursts = Some(
                    v.parse()
                        .map_err(|_| format!("config: bursts must be a number, got '{v}'"))?,
                )
            }
            "burst_on" | "burst-on" => cs.burst_on = Some(parse_bool(v, "burst_on")?),
            "notify" | "notify_on_cap" => cs.notify = Some(parse_bool(v, "notify")?),
            other => {
                return Err(format!(
                    "config: unknown key '{other}' (keys: interval, width, sl, sl_colors, sl_ascii, burst, burst_on, notify)"
                ))
            }
        }
    }
    Ok(cs)
}

fn parse_bool(v: &str, key: &str) -> Result<bool, String> {
    v.parse()
        .map_err(|_| format!("config: {key} must be true/false, got '{v}'"))
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
/// Splitting lives in `cmduse_core::dates`; this adds the CLI's range check.
pub fn parse_tz(s: &str) -> Option<i64> {
    let (sign, h, m) = cmduse_core::dates::parse_tz_parts(s)?;
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
  { \"interval_secs\": 5, \"bar_width\": 20, \"burst_enabled\": true,
    \"burst_samples\": 40, \"notify_on_cap\": true, \"statusline_template\": \"…\",
    \"statusline_colors\": true, \"statusline_ascii\": false }

daily/model/session read ~/.commandcode/projects offline (no API calls).
Dashboard needs: logged-in Command Code CLI (~/.commandcode/auth.json)"
}
