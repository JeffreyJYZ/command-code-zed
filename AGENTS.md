# AGENTS.md

Workspace: cmduse-core + cmd-usage CLI + Zed extension + opencode plugin,
single source of shared logic. Release line 0.6.0 (0.2–0.4 slots are
yanked-forever on crates.io from the old crate).

## Layout

```
Cargo.toml         workspace (members: core, cli, zed-ext)
core/              cmduse-core: shared pure logic, single source of truth
  plans.json       canonical plan table + name rules + monthly caps (see below)
  gating.json      canonical model categories + per-plan gating + hard-blocked
  conformance.json shared behavior vectors: Rust + TS tests both assert these
  build.rs         reads plans.json + gating.json → generated NAME_RULES/CAPS/
                   PLANS/GATE_* consts
  src/lib.rs       plan_name, plan_monthly_cap, money, compact, pct, rel_time,
                   elapsed_pct, pace_eta (10% gate lives here), gate/gate_allowed
                   (access decision + reason, mirrors opencode/src/access.ts)
  src/dates.rs     ISO/UTC date helpers (parse_iso_utc, civil_from_days,
                   now_secs, …)
  src/wire.rs      API wire DTOs shared by CLI + Zed (Credits/Window/SubData/…)
cli/               cmd-usage (bin `cmduse`), published to crates.io
  src/…            thin UI: api client, snapshot, ANSI rendering, reports, redraw
zed-ext/           command-code-usage Zed extension (WASM, markdown output)
  src/lib.rs       thin UI: HTTP via zed API, markdown window/plans render
opencode/          @jeffreyjyz/opencode-command-code TS plugin (no core crate;
                   imports ../../core/{plans,gating,conformance}.json directly)
```

## Core rules

- **Shared logic lives in `core/` ONLY.** A bug in plan table / window math /
  pace gate / dates gets ONE fix. Both `cli/` and `zed-ext/` depend on
  `cmduse-core` by path. UI-specific stuff stays local: ANSI constants +
  colored bars in cli, markdown formatting in zed-ext, bar glyphs differ per
  UI — do NOT move presentation into core.
- **Plan data lives in `core/plans.json`, gating data in `core/gating.json`,
  never in code.** `core/build.rs` bakes both into Rust consts; `opencode`
  imports the same files. Edit the JSON, not the generated consts or the TS.
  `opencode/scripts/extract-gating.ts` regenerates `gating.json` from the
  installed Command Code CLI bundle (`bun run extract`); the hand-probed
  `hardBlocked` entries live in that script.
- **Behavior vectors live in `core/conformance.json`.** Rust (`core` test) and
  TS (`opencode/test/conformance.test.ts`) both run it, so the two language
  ports of money/compact/rel_time/parse_iso_utc/plan_*/gating can't drift.
- `core` keeps adapters out: cli wraps `rel_time`/`elapsed_pct` to its
  `u64`-now signatures; zed-ext uses core's `Option<u64>` forms directly.
- Window caps (5-hour/weekly) come from the API `Window.cap` response, NOT
  derived. `plan_monthly_cap` is the only static table (monthly pool); window
  lengths are `core::FIVE_HOUR_SECS` / `core::WEEKLY_SECS`.

## Build & test

```sh
cargo test            # whole workspace (core + cli + zed-ext host tests)
cargo clippy --all-targets -- -D warnings
cargo build -p command-code-usage --target wasm32-wasip1 --release
cargo package -p cmduse-core --allow-dirty   # core ships plans.json+gating.json
cd opencode && bun test && bun run typecheck
```

## Publishing (NEVER without explicit user go)

- Order matters: `cmduse-core` first, then `cmd-usage`. `cli/Cargo.toml` dep
  is `{ path = "../core", version = "0.6.0" }` — path resolves locally, the
  `version` must already exist on crates.io for `cmd-usage` publish to work.
- **crates.io version slots are FOREVER.** 0.2.0–0.4.0 were published+yanked
  on old `cmd-usage` — you can never re-upload those numbers. Current 0.x
  release line is 0.6.0 (first free slot past the dead 0.2–0.4 range). Skip
  taken numbers, never fight the 400.
- Clean tree required (commit first, incl. Cargo.lock). Zed ext has NO
  release channel (local dev-install only).
- Homebrew after every cmd-usage release: `JeffreyJYZ/homebrew-tap`,
  `Formula/cmduse.rb` — bump version, url, sha256
  (`curl -sL https://static.crates.io/crates/cmd-usage/cmd-usage-<v>.crate | shasum -a 256`).
- README/AGENTS updated in the same commit.
- The opencode npm package (`opencode/package.json`) is versioned
  **independently** of the Rust workspace (0.1.1 vs 0.6.0) — intentional, not
  drift. Don't sync them.

## Learned-the-hard-way

API endpoints, cumulative-diff reports, TLS retry, watch-mode redraw rules
(frame's last line has NO trailing newline; frame-shrink = `\x1b[1B` +
`\x1b[2K\x1b[1B` + `\x1b[2K` + `\x1b[{prev-n}F`; test redraw bytes via
cli/src/main_tests.rs). Zed wasm: crate builds for `wasm32-wasip1`;
`extension.wasm` regenerated on dev-install.
