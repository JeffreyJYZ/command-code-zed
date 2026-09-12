# AGENTS.md

Workspace: cmduse-core + cmd-usage CLI + Zed extension + opencode plugin,
single source of shared logic. Release line 0.6.8 (0.2–0.4 slots are
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
  src/lib.rs       plan_name, plan_monthly_cap, money, compact, pct, duration,
                   rel_time, elapsed_pct, pace_eta (10% gate lives here),
                   monthly_window (cap−remaining clamp + period duration),
                   gate/gate_allowed, bare_model + canonical_model
                   (provider/alias normalization, mirror
                   opencode/src/{access,gating}.ts)
  src/dates.rs     ISO/UTC date helpers (parse_iso_utc, parse_tz_parts,
                   civil_from_days, iso_instant, now_secs, …)
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
  repo-root `scripts/extract-gating.ts` regenerates `gating.json` from the
  installed Command Code CLI bundle (`bun run extract`); the hand-probed
  `hardBlocked` entries live in that script. The file carries `extractedAt`
  + `cliVersion`; cli and opencode warn when the snapshot is >30d old, and
  both warn when the API returns a plan id no `plans.json` rule matches
  (the dashboard would otherwise silently show "Free" with no cap).
- **Behavior vectors live in `core/conformance.json`.** Rust (`core` test) and
  TS (`opencode/test/conformance.test.ts`) both run it, so the two language
  ports of money/compact/pct/duration/rel_time/parse_iso_utc/elapsed_pct/
  pace_eta/monthly_window/plan_*/gating/bare_model/canonicalize can't drift.
- `core` keeps adapters out: cli wraps `rel_time`/`elapsed_pct` to its
  `u64`-now signatures; zed-ext uses core's `Option<u64>` forms directly.
- Window caps (5-hour/weekly) come from the API `Window.cap` response, NOT
  derived. `plan_monthly_cap` is the only static table (monthly pool); window
  lengths are `core::FIVE_HOUR_SECS` / `core::WEEKLY_SECS`.
- **License text is sourced, never copied.** MIT files come from the OSI
  canonical (https://opensource.org/license/mit, SPDX `MIT`), fetched fresh —
  never copied from another component's file. All crates/packages are MIT; the
  body is identical, only the year/holder line is project-specific.
- **Docs always move with the code.** Any user-visible change updates the
  READMEs (`README.md`, `cli/README.md`), the `cli/cmduse.1` man page, and
  this file in the same commit — never a follow-up "docs" commit. Check for
  stale version refs and stale option/flag lists before committing.

## Build & test

```sh
cargo test            # whole workspace (core + cli + zed-ext host tests)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build -p command-code-usage --target wasm32-wasip1 --release
cargo package -p cmduse-core --allow-dirty   # core ships plans.json+gating.json
cd opencode && bun test && bun run typecheck
```

## Publishing (NEVER without explicit user go)

- Order matters: `cmduse-core` first, then `cmd-usage`. `cli/Cargo.toml` dep
  is `{ path = "../core", version = "0.6.7" }` — path resolves locally, the
  `version` must already exist on crates.io for `cmd-usage` publish to work.
- **crates.io version slots are FOREVER.** 0.2.0–0.4.0 were published+yanked
  on old `cmd-usage` — you can never re-upload those numbers. Current 0.x
  release line is 0.6.8 (first free slot past the dead 0.2–0.4 range). Skip
  taken numbers, never fight the 400.
- Clean tree required (commit first, incl. Cargo.lock). Zed ext has NO
  release channel (local dev-install only).
- Homebrew after every cmd-usage release: `JeffreyJYZ/homebrew-tap`,
  `Formula/cmduse.rb` — bump version, url, sha256
  (`curl -sL https://static.crates.io/crates/cmd-usage/cmd-usage-<v>.crate | shasum -a 256`).
- README/AGENTS updated in the same commit.
- The opencode npm package (`opencode/package.json`) is versioned
  **independently** of the Rust workspace (0.1.x vs 0.6.x) — intentional, not
  drift. Don't sync them.
- **npm publish is interactive: it fails from the agent shell** (`EOTP`, prints
  an auth URL). Build first (`cd opencode && bun run build`) so `dist/` is
  current, then the user runs plain `npm publish` themselves — it opens a
  browser to authenticate, no `--otp` needed. Never treat an `EOTP` failure as
  published — verify with `npm view @jeffreyjyz/opencode-command-code version`.

## Learned-the-hard-way

API endpoints, cumulative-diff reports, TLS retry, watch-mode redraw rules
(frame's last line has NO trailing newline; frame-shrink = `\x1b[1B` +
`\x1b[2K\x1b[1B` + `\x1b[2K` + `\x1b[{prev-n}F`; test redraw bytes via
cli/src/main_tests.rs). Zed wasm: crate builds for `wasm32-wasip1`;
`extension.wasm` regenerated on dev-install.
`--tz` offsets are **east-positive seconds** (`parse_tz("+05:30")=+19800`),
matching `tz_offset_suffix`; local = UTC + tz. All day/hour bucketing must use
that sign — a flipped `now - tz` silently shifts every local bucket (fixed in
0.6.2). `--tz` reaches the `--local` log path too (daily + hourly), not just
the account API. Report output goes through `render::color_enabled()`
(NO_COLOR + stdout tty); never emit raw SGR when piped. Route exact local hour boundaries through `dates::iso_instant` — do NOT
floor to a UTC hour, that breaks minute-bearing offsets like +05:30.
`pace_eta` returns **seconds** (a duration); format it with `core::duration`,
never `rel_time` — the latter expects an absolute reset epoch and renders any
small duration as "resetting…" (bug shipped in cli + zed until 0.6.2, and again
in the CLI statusline's `{5h_eta}`/`{wk_eta}` until 0.6.7). Any ETA text goes
through `duration`.

`money` rounds to cents with explicit multiply-round (`(v*100).round()/100`),
not `{:.2}`/`toFixed`: the two formats disagree at `.x5` ties (`0.125` →
half-even `$0.12` vs half-away `$0.13`; `2.675*100` rounds up to `267.5`, so
naive multiply gives `$2.68`). Both ports use the same multiply-round so the
conformance vectors pin them.

`--tz` account daily builds each `?since=` value as a UTC `Z` instant via
`dates::iso_instant`; never interpolate an offset suffix — `+` decodes as a
space server-side and silently corrupts the timestamp (fixed 0.6.7).

`gate`/`evaluateModelAccess` strip the provider qualifier with `bare_model`
*before* canonicalizing the incoming model, so a provider-qualified input
(`anthropic:claude-opus-5`) can't miss the category table and bypass
`hardBlocked`.

The update notice is drawn **inside** the watch frame (`render::update_box`),
never via mid-frame `eprintln`; `-1` sends it to stderr. The crates.io check is
cached 24h (`~/.cache/cmd-usage/last-check`, JSON `{checkedAt, latest}`) and the
cached version replays every run until upgraded. Gate it with
`check_updates=false` or `--dismiss-update` / `dismissed_update=<ver>`.
