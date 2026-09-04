pub struct Args {
    pub interval: Option<u64>,
    pub once: bool,
    pub plain: bool,
    pub bar_width: Option<usize>,
    pub help: bool,
    pub config_set: Option<(Option<u64>, Option<usize>)>,
}

pub fn parse_args() -> Args {
    let mut a = Args {
        interval: None,
        once: false,
        plain: false,
        bar_width: None,
        help: false,
        config_set: None,
    };
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
            "config" => {
                // config set [interval=<s>] [width=<n>]
                if let Some(sub) = it.next() {
                    if sub == "set" {
                        let mut interval = None;
                        let mut width = None;
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
                                    Ok(n) => interval = Some(n),
                                    Err(_) => {
                                        eprintln!("config: interval must be a number, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                "width" => match v.parse() {
                                    Ok(n) => width = Some(n),
                                    Err(_) => {
                                        eprintln!("config: width must be a number, got '{v}'");
                                        std::process::exit(2);
                                    }
                                },
                                other => {
                                    eprintln!("config: unknown key '{other}' (keys: interval, width)");
                                    std::process::exit(2);
                                }
                            }
                        }
                        a.config_set = Some((interval, width));
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

Usage: cmduse [options]

Options:
  -1, --once            Fetch once, print, exit (no watch)
  -p, --plain           No colors / no live redraw (for scripts, pipes)
  -i, --interval <s>    Refresh interval in seconds (default: config or 5)
  -w, --bar-width <n>   Progress bar width in chars (default: config or 20)
  -h, --help            This help

Config: ~/.config/cmd-usage/config.json
  { \"interval_secs\": 5, \"bar_width\": 20 }

  cmduse config set interval=<s> width=<n>   Save config options
  cmduse config set interval=10              Change just one

Requires: logged-in Command Code CLI (~/.commandcode/auth.json)"
}
