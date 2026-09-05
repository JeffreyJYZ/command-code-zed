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
cmduse -p -1                 # plain output, no ANSI (for scripts/pipes)
cmduse -i 30                 # refresh every 30s
cmduse -w 40                 # 40-char progress bars

cmduse daily --days 14       # account usage by day (all harnesses, from usage API)
cmduse daily --local         # CLI-logs-only (offline, misses other harnesses)
cmduse hourly --hours 6      # account usage by hour (default 24, max 168)
cmduse model                 # local usage by model
cmduse session               # local usage by project
cmduse statusline            # compact one-liner for prompts/tmux
cmduse daily --json          # JSON output (daily, hourly, model, session, statusline)
```

Burn-rate: windows show `on pace to hit cap in …` when the current spend rate projects hitting the cap before the window resets. ponytail: assumes flat spend rate; bursty sessions shift the ETA.

## Statusline

Template-driven via config. Placeholders:

| Placeholder | Shows |
|---|---|
| `{plan}` | plan name (GOAT, Pro, …) |
| `{credits}` | remaining monthly credits |
| `{cap}` | monthly cap |
| `{credits_bar}` | monthly usage bar |
| `{5h_bar}` `{5h_pct}` `{5h_used}` `{5h_cap}` | 5-hour window |
| `{wk_bar}` `{wk_pct}` `{wk_used}` `{wk_cap}` | weekly window |

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
  "statusline_template": "{plan} {credits}/{cap} · 5h {5h_bar} · wk {wk_bar}",
  "statusline_colors": true,
  "statusline_ascii": false
}
```

CLI flags override config. `cmduse config set interval=<s> width=<n> sl=<tpl> sl_colors=<bool> sl_ascii=<bool>`.

## Data sources

- **Dashboard / daily / hourly / statusline**: Command Code API with your account key. Daily and hourly cover **every harness** that used the key (CLI, Provider API, other agents).
- **model / session / daily --local**: local session logs at `~/.commandcode/projects` — offline, but only what the CLI recorded.

## Requirements

Logged-in [Command Code CLI](https://commandcode.ai) — reads your API key from `~/.commandcode/auth.json` (run `cmd login` if missing), or set `CMD_API_KEY`.

## Notes

- Window bars: green <70%, yellow 70–90%, red ≥90%, plus `LIMIT EXCEEDED` flag.
- Credits trend sparkline appears in watch mode after 2 refreshes.
- On Monthly caps: monthly pool is the plan total (e.g. $70 on GOAT). Docs describe per-model allowances, but the CLI and API meter one shared pool — verified empirically.
