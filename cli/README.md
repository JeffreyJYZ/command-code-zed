# cmduse

Live [Command Code](https://commandcode.ai) usage dashboard for your terminal.

Plan dashboard, account-wide usage reports (all harnesses), offline local reports, and a customisable statusline.

```
Command Code Usage · GOAT · active
Period ends 2026-09-27

Credits $59.12 / $70.00 monthly · $0.00 purchased · $0.00 free

Usage windows
 Monthly   15.1% ━━╱╱╱╱╱╱╱╱  $10.56 / $70.00 · resets in 22d 9h · window 28% elapsed
 5-hour     1.5% ╱╱╱╱╱╱╱╱╱╱  $0.21 / $14.00 · resets in 4h 58m · window 0% elapsed
 Weekly    15.8% ━╱╱╱╱╱╱╱╱╱  $5.54 / $35.00 · resets in 6d 5h · window 11% elapsed · on pace to hit cap in 4d 3h

This billing period
 Requests 856 · Cost $10.69 · Tokens 89.5M in / 265.7K out · Success 100%
```

## Install

```sh
brew install JeffreyJYZ/tap/cmduse   # macOS (Homebrew)
cargo install cmd-usage              # any platform with Rust
```

## Usage

```sh
cmduse                       # live dashboard, redraws in place (default every 5s)
cmduse -1                    # one-shot fetch, print, exit
cmduse -V                    # print version
cmduse -p -1                 # plain output, no ANSI (for scripts/pipes)
cmduse -i 30                 # refresh every 30s
cmduse -i 5m                 # duration suffixes: s, m, h, d
cmduse -w 40                 # 40-char progress bars
cmduse watch                 # explicit watch mode (same as bare cmduse)
man cmduse                   # full behavior spec (brew installs the man page)

cmduse daily --days 14       # account usage by day (all harnesses, from usage API)
                             # --days max 365, fetched 8-at-a-time
cmduse daily --local         # CLI-logs-only (offline, misses other harnesses)
cmduse daily --tz +05:30     # bucket by a fixed UTC offset instead of UTC
cmduse daily --local --tz +05:30  # ...the local-log path honors --tz too
cmduse hourly --hours 6      # account usage by hour (default 24, max 168)
cmduse hourly --local        # hourly from local CLI logs (offline)
cmduse model                 # local usage by model
cmduse session               # local usage by project
cmduse models                # live model list from the Command Code API
cmduse models --gated        # ...only models the current plan allows
cmduse models --gated --json # every model annotated with allowed + reason
cmduse plans                 # plan comparison table (marks your plan)
cmduse statusline            # compact one-liner for prompts/tmux
cmduse daily --json          # JSON output (daily, hourly, model, session, models, statusline, plans, -1)
cmduse daily --csv           # CSV output for daily/hourly/model/session
```

Color: SGR escapes are suppressed when stdout is not a terminal or when
`NO_COLOR` is set non-empty. `--plain` forces plain text; reports have no
`--plain` — pipe them or use `--json`/`--csv`.

`-1 --json` emits a single dashboard object (plan, credits, both windows,
billing summary). `--gated --json` on `models` emits every model with
`allowed` and `reason` instead of filtering.

GNU forms are accepted: `--interval=5m`, `--days=14`, and attached short values
like `-i30` / `-w40`. `--last`/`-l` are aliases for `--days`.

Cap alerts: in watch mode a desktop notification fires once when a window
crosses into overflow (macOS `osascript`, Linux `notify-send`). Disable with
`cmduse config set notify=false`.

Burn-rate: windows show `on pace to hit cap in …` when the current spend rate projects hitting the cap before the window resets, and only once the window is ≥10% elapsed (flat-rate projection is unreliable early). ponytail: assumes flat spend rate; bursty sessions shift the ETA.

Watch mode: a `spend bursts (N samples)` sparkline shows $ spent per refresh.
It is on by default, hidden while idle (all-zero deltas), and its sample count
is configurable. Session-only: a fresh run starts a fresh trend (no stale
data from earlier runs):

```sh
cmduse config set burst_on=false      # turn the sparkline off
cmduse config set burst=80            # 80 samples (5–240)
cmduse -b 120                         # same for one run
```

## Statusline

Template-driven via config. Placeholders:

| Placeholder | Shows |
|---|---|
| `{plan}` | plan name (GOAT, Pro, …) |
| `{credits}` | remaining monthly credits |
| `{cap}` | monthly cap |
| `{credits_bar}` | monthly usage bar |
| `{5h_bar}` `{5h_pct}` `{5h_used}` `{5h_cap}` `{5h_eta}` | 5-hour window (eta = pace text, empty when none) |
| `{wk_bar}` `{wk_pct}` `{wk_used}` `{wk_cap}` `{wk_eta}` | weekly window |

```sh
cmduse config set sl="{plan} {credits}/{cap} 5h:{5h_pct} wk:{wk_pct}"
cmduse config set sl="{credits_bar}"           # just one bar
cmduse config set sl_colors=false              # strip ANSI
cmduse config set sl_ascii=true                # #--- bars instead of ━╱╱
```

Multi-line templates work (newlines allowed). Unknown placeholders are dropped. Wire into your shell prompt or tmux status:

```sh
# .zshrc / tmux status-right
status() { cmduse statusline 2>/dev/null; }
```

`CMD_API_KEY` env var overrides the stored key — point `cmduse` at any account without touching `~/.commandcode/auth.json`.

## Config

`~/.config/cmd-usage/config.json`:

```json
{
  "interval_secs": 5,
  "bar_width": 20,
  "burst_enabled": true,
  "burst_samples": 40,
  "notify_on_cap": true,
  "statusline_template": "{plan} {credits}/{cap} · 5h {5h_bar} · wk {wk_bar}",
  "statusline_colors": true,
  "statusline_ascii": false,
  "check_updates": true,
  "dismissed_update": null
}
```

CLI flags override config. `cmduse config set interval=<s> width=<n> burst_on=<bool> burst=<n> notify=<bool> sl=<tpl> sl_colors=<bool> sl_ascii=<bool> update_check=<bool> dismissed_update=<ver>`.

## Updates

Once per day (cache at `~/.cache/cmd-usage/last-check`) the dashboard checks
crates.io; when a newer `cmd-usage` exists it shows a boxed notice inside the
watch frame (stderr for `-1`). The cached version is replayed every run, so the
reminder persists until you upgrade. Dismiss or disable:

```sh
cmduse --dismiss-update              # hide this version until a newer one
cmduse config set update_check=false # never check
cmduse config set dismissed_update=0.6.8
```

## Data sources

- **Dashboard / daily / hourly / statusline**: Command Code API with your account key. Daily and hourly cover **every harness** that used the key (CLI, Provider API, other agents).
- **model / session / daily --local**: local session logs at `~/.commandcode/projects` — offline, but only what the CLI recorded.

## Requirements

Logged-in [Command Code CLI](https://commandcode.ai) — reads your API key from `~/.commandcode/auth.json` (run `cmd login` if missing), or set `CMD_API_KEY`.

## Notes

- Window bars: green <70%, yellow 70–90%, red ≥90%, plus `LIMIT EXCEEDED` flag.
- Report tables show cache read and cache write separately; `daily` is bucketed in UTC unless `--tz`, and `--local` honors `--tz`; token totals in the model/session `Tokens` column include both cache columns.
- Spend-burst sparkline appears in watch mode after 2 refreshes (bars = $ spent between refreshes, ~3 min of history at 5s interval, capped at 40 samples; tall = burst, flat = idle).
- On Monthly caps: monthly pool is the plan total (e.g. $70 on GOAT). Docs describe per-model allowances, but the CLI and API meter one shared pool — verified empirically.
