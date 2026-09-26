# AGENTS.md

Workspace: cmduse-core + cmd-usage CLI (+ built-in MCP server) + opencode
plugin, single source of shared logic. Two independent version lines:
cmd-usage 0.6.x and cmduse-core 1.x (0.2–0.4 slots are yanked-forever on
crates.io from the old cmd-usage crate).

## Layout

```
Cargo.toml         workspace (members: core, cli)
core/              cmduse-core: pure logic + canonical data, no I/O
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
  src/reports.rs   usage aggregation: Usage/Totals, day+hour bucketing in a
                   fixed offset, cumulative-difference math, local bucket fold
  src/wire.rs      API wire DTOs (Credits/Window/SubData/UsageSummary/…)
cli/               cmd-usage, published to crates.io
  src/lib.rs       `pub fn run()` — the CLI body; `src/main.rs` and
                   `src/bin/cmdusedev.rs` are thin entry points (the dev twin
                   lets a local build avoid shadowing the installed `cmduse`)
  src/…            the application: arg parsing, HTTP client + retry, snapshot
                   assembly, report fetching, ANSI/markdown rendering, config,
                   update check, live watch/redraw loop
  src/reports.rs   I/O only: session-JSONL walk + account API pool; the math
                   comes from core::reports
  src/mcp.rs       `cmduse mcp` MCP stdio server (hand-rolled JSON-RPC; tools
                   reuse the same output helpers as the CLI subcommands)
opencode/          @jeffreyjyz/opencode-command-code TS plugin (dual opencode
                   v1 (server()) + v2 (setup()) entrypoints; no core crate;
                   imports ../../core/{gating,conformance}.json; usage
                   rendering delegates to the cmduse CLI via src/cli.ts)
  src/v2.ts        v2 half: providers + /connect integration + live model list
                   (static seed, then fetch + gating, re-fetched every 30 min —
                   unlike opencode-cmd-provider, which bakes its list)
  src/tui.tsx      TUI half (package ./tui export), two hosts one module:
                   v1 `tui(api)` registers `sidebar_content`, v2 `setup(ctx)`
                   claims `sidebar.content`; v2 also keeps the /cmd-usage slash
                   command (spawns cmduse client-side; the server half's
                   synthetic messages are not TUI-visible)
  src/sidebar/rows.ts   pure row builder for the sidebar (usage + model rows)
  src/sidebar/data.ts   spawns: cmduse for usage (polled), mpc --json for the
                   per-model catalog (disk-cached 6h; mpc scrapes live docs)
  scripts/build-tui.ts  builds dist/tui.js with @opentui/solid's transform
```

## Core rules

- **Pure logic and canonical data live in `core/`; the CLI is the application.**
  A bug in plan table / window math / pace gate / dates / usage aggregation
  gets ONE fix. `cli/` depends on `cmduse-core` by path and owns I/O only:
  HTTP, filesystem, terminal, and formatting. Presentation stays local (ANSI
  constants, colored bars, tables) — do NOT move it into core.
- **Plan data lives in `core/plans.json`, gating data in `core/gating.json`,
  never in code.** `core/build.rs` bakes both into Rust consts; `opencode`
  imports the same files. Edit the JSON, not the generated consts or the TS.
  repo-root `scripts/extract-gating.ts` regenerates `gating.json` from the
  installed Command Code CLI bundle (`bun run extract`); the hand-probed
  `hardBlocked` entries live in that script. The file carries `extractedAt`
  + `cliVersion`; cli and opencode warn when the snapshot is >30d old, and
  both warn when the API returns a plan id no `plans.json` rule matches
  (the dashboard would otherwise silently show "Free" with no cap).
- **The per-model catalog lives in `opencode/src/catalog.ts`, generated.**
  The listing API (`/provider/v1/models`) returns no capabilities and no rates,
  so the only source is the official CLI package: `models.md` (context, efforts,
  $/1M in/out/cache-read, plus cache-write on Anthropic models) and `dist/cli.mjs`
  (`inputModalities`). Regenerate with `bun scripts/extract-catalog.ts` (in
  `opencode/`, or `bun run extract:catalog`) after a Command Code release; it
  fetches both, stamps the version, and warns about modalities-only ids the docs
  have not priced yet. Models absent from the table are text-only with no price —
  never assume vision or invent a rate. Two catalogue notes: 1.65 inverted the
  modality default, so a model accepts images unless it is in the bundle's `Rr`
  text-only denylist — the generator reads that set (explicit per-model records
  still win) and mirrors the CLI's own `supportsVision`; and the same models.md
  carries the `Min plan` column, which `plans.md` names as the access rule and
  which the sidebar reports as its `Min plan` row.
- **The plugin halves are plain objects; `@opencode/*` is dev-only.** opencode
  decodes the default export against its own `Plugin` interface (both `define`
  helpers are the identity function), so `src/v2.ts` / `src/tui.tsx` export
  `{ id, setup }` literals typed by `import type`, and `src/index.ts` defines its
  v1 tool inline (`tool()` there is also identity) with `zod` for the args shape.
  `@opencode/plugin` + `@opencode-ai/plugin` sit in devDependencies only: the
  built `dist/index.js` must import nothing but node builtins, which is what keeps
  a fresh opencode start from installing their ~270 MB graph (`@opencode/ai`,
  `effect`, `@opentelemetry`, `@aws-sdk`) before the provider appears.
- **The picker must never wait on the live list.** Two traps found the hard way:
  (1) `loadModels` falls back to "show everything" when the billing API is
  unreachable, and that ungated list swings between ~61 and ~82 ids with network
  luck — so an id-diff check sees a change on every start and fires a transform
  anyway. `ModelSplit.gated` now marks that fallback and v2 ignores such a list
  entirely (no cache write, no transform). (2) Even a legitimately changed list
  must not transform on the first pass after start: the first deferred refresh
  warms the cache only, and 30-minute ticks may update the registry.
- **The live model list is cached to `$XDG_CACHE_HOME/command-code/models.json`.** A
  warm start merges it during registration, so the picker is fresh at ~1ms and
  the background refresh usually finds nothing to change. The refresh only calls
  `provider.transform` when the id sets differ (`idsDiffer`) and is deferred 3s
  past setup: a transform landing while the TUI paints makes the host re-publish
  provider/model state, which reads as "the UI waited for the fetch". The listing
  API itself takes ~2.5s; that is fine as long as it stays off the paint path.
- **Startup timing goes to `$XDG_CACHE_HOME/command-code/startup.log`.** A
  plugin's `console.log` runs in opencode's server process and is NOT captured by
  its log file, so `src/startupLog.ts` appends one line per start:
  `setup: key=…ms register=…ms connection=…ms refresh=…ms models=…`. `register`
  is the phase that gates the picker; if it grows, look at plugin load, not setup.
- **`core/gating.json`'s category scrape is still on the 1.38.2 anchors.** The
  1.65 bundle moved the `Vr`/`zr`/`Kr` declarations, so `scripts/extract-gating.ts`
  now prefers the published bundle (unpkg, then jsdelivr — unpkg 500s on some
  versions; a local install is the offline fallback) and fails loud on the old
  anchors. Re-anchoring is its own task; until then plan gating falls back to
  "allow" for the newest models (the API still enforces) and the tier row simply
  omits those models — prefer `Min plan` when both are available.
- **Behavior vectors live in `core/conformance.json`.** Rust (`core` test)
  asserts them all. Since plugin 0.2.0 the TS port (`opencode/test/
  conformance.test.ts`) covers only the still-ported model-gating layer
  (bare_model/canonicalize/gating) — money/compact/pct/duration/rel_time/
  parse_iso_utc/elapsed_pct/pace_eta/monthly_window/plan_* have a single
  implementation (Rust core) because the opencode plugin spawns the cmduse
  CLI for all usage rendering instead of porting that logic.
- `core` keeps adapters out: cli wraps `rel_time`/`elapsed_pct` to its
  `u64`-now signatures.
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

## Consumers (sibling repo, same owner)

`mpc` (`~/dev/clis/oc-cmd-compare`) reads this workspace: it shells out to `cmduse`, and takes the
per-model mix from opencode's own message store (`~/.local/share/opencode/opencode.db`) rather than
from this plugin. Stable contracts, not incidental output:

| contract | consumer |
| --- | --- |
| `cmduse plans --json` — plan name/price/credits/windows | mpc plan table |
| `cmduse -1 --json` — `summary.requests`/`summary.cost`, `periodEnd` | mpc coverage line + billing window |
| `cmduse model --json [--since ISO]` — `{source, since, models:{id: totals}}` | mpc `--usage` |
| `mpc --json` — `{rows:[{key, name, cc:{allowance, pricing, ability, tps}}]}` | the sidebar's model rows (allowance, rates, Intelligence, Tok/s) |

Changing any of those shapes means updating mpc in the same effort; `CMDUSE_BIN` lets mpc test a
`cmdusedev` build. Local commits only — never publish or push without explicit go.

## Build & test

```sh
cargo test            # whole workspace (core + cli host tests)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo package -p cmduse-core --allow-dirty   # core ships plans.json+gating.json
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | cargo run -p cmd-usage -- mcp   # MCP smoke
cd opencode && bun test && bun run typecheck
```

Tests must be hermetic: CI (Ubuntu) has no `cmduse` binary and no homebrew
prefix, so anything that shells out to the real CLI needs a skip guard
(`test.skipIf`) or an injected candidate list — a passing local run proves
nothing about CI. Mirror CI before committing: `cargo fmt --all -- --check`,
`cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`,
`cargo package -p cmduse-core --allow-dirty`, then the opencode job's
`bun install && bun test && bun run typecheck && bun run build`.

## Publishing (NEVER without explicit user go)

- **Versions are independent: `cmduse-core` is on its own `1.x` line; the CLI
  is 0.6.x.** They are NOT a pair — do not try to read one from the other, and
  never "sync" them. Core bumps only when its own API changes (breaking → major,
  additive → minor, fix → patch); the CLI bumps per release as usual.
  `cli/Cargo.toml` depends on `{ path = "../core", version = "1" }`, so a new
  core minor/patch needs no CLI edit.
- **Core's major always leads the CLI's.** When the CLI enters a major band
  (e.g. CLI 1.0.0), publish `cmduse-core` at the next major (2.0.0) as a
  line-separation release — no API change, note it in the description. This
  keeps the two numbers from ever sitting in the same band and being read as a
  pair again.
- Publish order when core changed: `cmduse-core` first, then `cmd-usage` —
  `cargo package -p cmd-usage` verifies against the *published* core, so a
  core API the registry doesn't have yet fails the tarball build with compile
  errors you won't see locally. If core did not change, publish the CLI alone.
- **crates.io version slots are FOREVER.** 0.2.0–0.4.0 were published+yanked
  on old `cmd-usage` — you can never re-upload those numbers. Current 0.x
  release line is 0.6.12 (first free slot past the dead 0.2–0.4 range). Skip
  taken numbers, never fight the 400.
- Clean tree required (commit first, incl. Cargo.lock).
- Homebrew after every cmd-usage release: `JeffreyJYZ/homebrew-tap`,
  `Formula/cmduse.rb` — bump version, url, sha256
  (`curl -sL https://static.crates.io/crates/cmd-usage/cmd-usage-<v>.crate | shasum -a 256`).
- README/AGENTS updated in the same commit.
- The opencode npm package (`opencode/package.json`) is versioned
  **independently** of the Rust workspace (0.2.x vs 0.6.x) — intentional, not
  drift. Don't sync them. 0.2.0 added opencode v2 support (dual v1+V2
  entrypoint, `@opencode/plugin` + `@opencode-ai/plugin` deps, both external
  in the bun build; v1 floor is the 1.18.29 object entrypoint); 0.2.x also
  ships a TUI half (`./tui` export → `src/tui.ts`, `solid-js` devDep for the
  test runner only — opencode resolves the TUI import at runtime). 0.2.5 adds
  the session sidebar (`src/sidebar/*`, `src/tui.tsx`) which consumes
  `mpc --json` for model allowance/rates/benchmarks. 0.2.8 fixed the real
  "black line" / "fetching usage…" artifact: cmduse's `snapshot()` paints a
  spinner straight to `/dev/tty` (piping stdout/stderr does not stop it), so the
  sidebar's poll overpainted the TUI. Fix: spawn cmduse `detached` (no
  controlling terminal → the `/dev/tty` open fails) and poll only while the
  session is on one of our models. cmduse 0.6.12 also stops the spinner unless
  it is driving the live dashboard on a tty. 0.2.7's app-slot `void` change was
  a red herring — the claim already resolves to `null` safely. 0.2.9 adds the
  active model's period usage to the panel (`src/sidebar/usageDb.ts`): a
  read-only `bun:sqlite` scan of opencode's own store, since cmduse's account
  API has no per-model dimension. Spend is shown only when the harness priced
  it — CommandCode is subscription-billed, so opencode records cost 0 there.
  0.2.10 fixed every model advertising as text-only: capabilities now come from
  the generated `catalog.ts` (0.2.9 and earlier hardcoded `attachment: false`
  + `input.image: false`, so no image could be attached even to `*-vision-*`).
  0.2.11 dropped the `@opencode/*` runtime deps — the halves are plain objects
  and type-only imports, so a fresh start no longer installs @opencode/plugin's
  ~511 MB graph (@opencode/ai, effect, @opentelemetry, @aws-sdk) before the
  provider appears. It also takes context/efforts/$-rates from the same generated
  catalog (opencode prices CommandCode models now), adds effort variants on v2,
  and shows percent elapsed on the sidebar's windows. 0.2.10 was never published
  (0.2.9 → 0.2.11). 0.3.0 owns the streaming instead of borrowing opencode's
  provider packages: `src/provider/` is an AI SDK v3 LanguageModel over
  `/provider/v1/{messages,chat/completions}` (pure `wire.ts` conversion + a
  `stream.ts` state machine), handed to v2 via `ctx.aisdk.hook("sdk", …)` and to
  v1 by pointing `npm` at our own package pinned to its exact version (v1's
  loader imports the module and takes the first `create*` export —
  `createCommandCode`). Not yet ported from the reference implementation: retry
  ladders, `pause_turn` continuation, and the legacy `/alpha/generate` fallback
  Go accounts need when the provider API plan-gates them. 0.3.1 made the
  provider appear in /model as fast as the seed allows: `setup()` registers the
  provider + snapshot + aisdk hooks before it touches credentials (the
  integration connection lookup can take seconds and only upgrades
  `activation`/`apiKey` afterwards), the live list **merges** over the snapshot
  (`mergeModels`: add + update, never remove, so a gated or partial response
  cannot make a model vanish), and one always-on line reports where a slow start
  went: `[command-code] setup: key=…ms register=…ms connection=…ms refresh=…ms
  models=…`. Also: `<1%` instead of `0%` for windows just started, an unpkg
  fallback in `scripts/extract-gating.ts` (a local CLI still wins — this machine
  has 1.38.2 installed while 1.65.0 is published, so a real refresh must use the
  published bundle or a newer CLI), and a README tip to pin the plugin specifier
  so opencode stops re-resolving `@latest` on every start.
  0.2.4 is burned: its
  tarball never landed, so it is deprecated and skipped. 0.3.2 added the min-plan
  row, CLI-accurate vision, a readable startup log and the CDN fallbacks; 0.3.3
  cached the live list to disk so a warm start never waits on the listing API;
  0.3.4 stopped the refresh from transforming after paint at all — the ungated
  fallback list swings in size with network luck, which defeated the id-diff and
  made every start look changed.
- **npm publish is interactive: it fails from the agent shell** (`EOTP`, prints
  an auth URL). Build first (`cd opencode && bun run build`) so `dist/` is
  current, then the user runs plain `npm publish` themselves — it opens a
  browser to authenticate, no `--otp` needed. Never treat an `EOTP` failure as
  published — verify with `npm view @jeffreyjyz/opencode-command-code version`.
- **Verify a release with `bun scripts/verify-release.ts <version> [--expected <sha1>]`.**
  It polls the packument and the tarball, reports `latest` / version / tarball
  status in one line, and (given a local `npm pack` shasum) fails on a mismatch
  or a tarball that disagrees about its version. Keep it in the release loop
  rather than eyeballing the registry.
- **A successful publish is asynchronous, in two visible stages.** The CLI
  returns `PUT 202` ("Your package is being processed") and exit 0 immediately,
  but the registry updates the **packument** (so `dist-tags.latest` and
  `versions[<v>]` appear) about a minute later, and serves the **tarball** at
  `…/-/opencode-command-code-<v>.tgz` several minutes after that — measured on
  this package: 0.2.7 ~4.5 min, 0.2.8 ~4.5 min, 0.2.11 ~5 min, 0.3.0 ~5 min.
  During the gap, `latest` already points at the new version while its tarball
  still 404s, so a consumer that resolves `@latest` in that window installs
  nothing (this is what burned 0.2.4). Verify in this order: packument shows the
  version → poll the tarball URL until 200 → compare
  `shasum -a 1` against a local `npm pack` → only then restart opencode.
- After publishing a plugin version, opencode may keep resolving the previous
  one: its per-package install cache (`~/.cache/opencode/npm/<pkg>@latest/`) is
  built from a **cached npm packument**, which can lag the registry for
  minutes. Bump steps: `npm cache clean --force`, `rm -rf
  ~/.cache/opencode/npm/@jeffreyjyz/opencode-command-code@latest`, then
  `opencode service restart` and confirm the version in `opencode plugin list`.
  (`npm install --prefer-online` proves the registry has the new version.)
  Temporary workaround if the cache is stubborn: pin the specifier to the exact
  version in the user's `plugins` array.

## Learned-the-hard-way

The sidebar's TUI bundle must be compiled with `@opentui/solid`'s solid transform
(`scripts/build-tui.ts`), not plain `bun build`: a plain JSX emit evaluates props at
element-creation time, so the panel freezes at mount and never repaints when the session model
changes. `@opentui/*` and `solid-js` stay external and the slice stays one bundle because both
TUI hosts rewrite the entry's imports to their own module instances.

API endpoints, cumulative-diff reports, TLS retry, watch-mode redraw rules
(frame's last line has NO trailing newline; frame-shrink = `\x1b[1B` +
`\x1b[2K\x1b[1B` + `\x1b[2K` + `\x1b[{prev-n}F`; test redraw bytes via
cli/src/main_tests.rs). MCP: hand-rolled stdio JSON-RPC (newline-delimited);
notifications (no `id`) get NO response; tool errors are `isError: true`
results, never JSON-RPC errors; stdout is protocol-only — anything printed by
a tool body would corrupt the stream, so tool text goes through the result
envelope. `--tz` offsets are **east-positive seconds** (`parse_tz("+05:30")=+19800`),
matching `tz_offset_suffix`; local = UTC + tz. All day/hour bucketing must use
that sign — a flipped `now - tz` silently shifts every local bucket (fixed in
0.6.2; the sign tests now live in `core/src/reports.rs`, together with the
bucketing they guard). `--tz` reaches the `--local` log path too (daily +
hourly), not just the account API. Report output goes through `render::color_enabled()`
(NO_COLOR + stdout tty); never emit raw SGR when piped. Route exact local hour boundaries through `dates::iso_instant` — do NOT
floor to a UTC hour, that breaks minute-bearing offsets like +05:30.
`pace_eta` returns **seconds** (a duration); format it with `core::duration`,
never `rel_time` — the latter expects an absolute reset epoch and renders any
small duration as "resetting…" (bug shipped in cli + zed until 0.6.2 — zed
since deleted — and again
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
