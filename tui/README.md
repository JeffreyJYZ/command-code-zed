# cmduse

Live [Command Code](https://commandcode.ai) usage dashboard for your terminal.

Shows your plan (Go, GOAT, Pro, Provider, Max 10x/20x, Team Pro), remaining credits, rolling 5-hour and weekly usage windows with reset countdowns, and billing-period request stats.

```
Command Code Usage · GOAT · active
Period ends 2026-09-27

Credits $63.03 monthly · $0.00 purchased · $0.00 free

Rolling windows
 5-hour    4.2% ━╱╱╱╱╱╱╱╱  $0.59 / $14.00 · resets in 3h 32m
 Weekly    1.7% ━╱╱╱╱╱╱╱╱  $0.59 / $35.00 · resets in 6d 22h

This billing period
 Requests 511 · Cost $7.54 · Tokens 34.0M in / 193.1K out · Success 100%
```

## Install

```sh
cargo install cmd-usage
```

## Usage

```sh
cmduse              # live dashboard, redraws in place (default every 5s)
cmduse -1           # one-shot fetch, print, exit
cmduse -p -1        # plain output, no ANSI colors (for scripts/pipes)
cmduse -i 30        # refresh every 30s
cmduse -w 40        # 40-char progress bars
```

## Config

`~/.config/cmd-usage/config.json`:

```json
{ "interval_secs": 5, "bar_width": 20 }
```

CLI flags override config. Defaults: 5s interval, 20-char bars.

## Requirements

Logged-in [Command Code CLI](https://commandcode.ai) — reads your API key from `~/.commandcode/auth.json` (run `cmd login` if missing).

## Colors

Window bars: green <70%, yellow 70–90%, red ≥90%, plus `LIMIT EXCEEDED` flag.
