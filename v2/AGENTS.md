# AGENTS.md (v2 — 0.2.0 restructure)

0.2.0 branch: Cargo workspace + opencode plugin, single source of shared logic. Old 0.1.x tree frozen at repo root (`tui/`, `src/`, root `extension.toml`, `AGENTS.md`) — that tree only gets 0.1.x bugfixes, NEVER edit it for 0.2 work.

## Layout

```
Cargo.toml         workspace (members: core, cli, zed-ext)
core/              cmduse-core: shared pure logic, single source of truth
  src/lib.rs       plan_name, plan_monthly_cap, money, compact, pct, rel_time,
                   elapsed_pct, pace_eta (10% gate lives here)
  src/dates.rs     ISO/UTC date helpers (parse_iso_utc, civil_from_days, …)
cli/               cmd-usage (bin `cmduse`), published to crates.io
  src/…            thin UI: api client, snapshot, ANSI rendering, reports, redraw
zed-ext/           command-code-usage Zed extension (WASM, markdown output)
  src/lib.rs       thin UI: HTTP via zed API, markdown window/plans render
opencode/          @jeffreyjyz/opencode-command-code TS plugin (unchanged, no core dep)
```

## Core rules

- **Shared logic lives in `core/` ONLY.** A bug in plan table / window math /
  pace gate / dates gets ONE fix. Both `cli/` and `zed-ext/` depend on
  `cmduse-core` by path. UI-specific stuff stays local: ANSI constants +
  colored bars in cli, markdown formatting in zed-ext, bar glyphs differ per
  UI — do NOT move presentation into core.
- `core` keeps adapters out: cli wraps `rel_time`/`elapsed_pct` to its
  `u64`-now signatures; zed-ext uses core's `Option<u64>` forms directly.
- Window caps (5-hour/weekly) come from the API `Window.cap` response, NOT
  derived. `plan_monthly_cap` is the only static table (monthly pool).

## Build & test

```sh
cargo test            # whole workspace (core + cli + zed-ext host tests)
cargo clippy --all-targets -- -D warnings
cargo build -p command-code-usage --target wasm32-wasip1 --release
cd opencode && bun test && bun run typecheck   # plugin unchanged
```

## Learned-the-hard-way (carried over)

Everything in root `AGENTS.md` "Critical knowledge" applies — API endpoints,
cumulative-diff reports, TLS retry, watch-mode redraw rules (frame's last
line has NO trailing newline; frame-shrink = `\x1b[1B` + `\x1b[2K\x1b[1B` +
`\x1b[2K` + `\x1b[{prev-n}F`; test redraw bytes via main_tests.rs). Zed wasm:
crate builds for `wasm32-wasip1`; `extension.wasm` regenerated on dev-install.
