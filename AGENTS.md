# AGENTS.md

Command Code usage tooling: a Zed extension (WASM slash command) and `cmduse`, a standalone Rust CLI that renders Command Code plan/usage data in the terminal.

## Layout

```
extension.toml      Zed extension manifest (schema_version 1)
src/lib.rs          Zed extension: /cmd-usage slash command, assistant-panel markdown output
tui/                cmduse CLI crate (published as `cmd-usage` on crates.io, bin = `cmduse`)
  src/main.rs       entry: arg dispatch, watch loop (in-place redraw), statusline cmd
  src/cli.rs        arg parsing (Args, SubCmd, ConfigSet), usage text
  src/api.rs        API client: auth key read, GET helper (5-retry transient matcher incl 429/5xx), endpoint structs
  src/dates.rs      ISO/UTC date helpers: parse_iso_utc, civil_from_days, today_utc, day_shift, hour label
  src/snapshot.rs   fetch orchestration: parallel endpoints, spinner thread (single snapshot() fn)
  src/render.rs     dashboard rendering (ANSI), bars, rel_time, burn rate, sparkline, re-exports dates::parse_iso_utc
  src/reports.rs    local JSONL parser + account-wide daily/hourly (cumulative-diff); HTTP via api::summary_since
  src/report_render.rs  statusline template engine + report tables (local/account/hourly/model/session)
  src/config.rs     config load/save, validation, XDG path
  src/render_tests.rs / config_tests.rs / cli_tests.rs / main_tests.rs   unit tests
.github/workflows/ci.yml   CI: tui build/test/clippy -D warnings + Zed ext wasm build
```

## Critical knowledge (learned the hard way)

### Command Code API (api.commandcode.ai)

- Auth: `Authorization: Bearer <key>`; key lives in `~/.commandcode/auth.json` (`apiKey` field). `CMD_API_KEY` env var overrides (multi-account/testing).
- Working endpoints (all GET, all `alpha/` prefix):
  - `/alpha/billing/credits` — `{credits:{monthlyCredits,purchasedCredits,freeCredits}, windowLimits:{fiveHour:{used,cap,exceeded,resetAt},weekly:{...}}}`. resetAt = epoch ms (f64). `monthlyCredits` = REMAINING, not spent.
  - `/alpha/billing/subscriptions` — `{data:{status, planId, currentPeriodStart, currentPeriodEnd}}`. planId like `individual-goat`.
  - `/alpha/usage/summary?since=<ISO>` — cumulative totals from that instant to now: `totalCount, totalCost, totalTokensIn/Out, successRate, periodBasis`. **No per-model split, no cache tokens, no per-request history.** Query params like groupBy are silently ignored.
  - `/alpha/whoami` — user info.
- undocumented routes return 404 JSON; probe with curl when hunting endpoints. CLI bundle (find via pnpm store path in `~/Library/pnpm/bin/cmd` shim → `command-code/dist/cli.mjs`) greps reveal endpoint paths and client logic.
- Per-day usage = `cum(dayStart) - cum(nextDayStart)` (cumulative-diff trick). Same for hourly. Fetch boundaries in parallel threads; ureq errors retry once (TLS blips happen).
- **Monthly caps: one shared pool, NOT per-model wallets.** Docs claim per-model allowances ($40 GLM-5.3 Flash etc); verified empirically: CLI's `getPlanTotalCredits(planId)` returns flat pool (GOAT=70), API has no model-level accounting, live remaining decrements past would-be model caps. Plan category gating exists (GOAT=opensource only) but that's model *access*, not separate wallets.
- Plan table (monthly pool): Go $10, GOAT $70, Pro $80, Provider PAYG, Max 10x $150, Max 20x $300, Team Pro $40.
- 5-hour/weekly caps by plan: Go 3/6, GOAT 14/35, Pro 16/40, Max10x 45/90, Max20x 90/180, Team 12/24.
- API latency varies wildly (0.3s–4s for 3 calls). Always parallelize endpoint fetches. One retry on connection errors.
- `~/.commandcode/projects/<flattened-path>/<session-id>.jsonl` = CLI session logs. Line types: `session` (has cwd), `message`. Assistant messages carry top-level `usage` (`inputTokens`,`outputTokens`,`cacheReadTokens`,`cacheWriteTokens`,`costUsd` — camelCase) and `model`, `timestamp` (ISO Z). Skip `*.meta.json` and `*checkpoints*`. Local logs only contain CLI sessions — other harnesses' usage exists ONLY via API.

### cmduse architecture

- Watch mode: true in-place redraw. Frame's LAST line has NO trailing newline so cursor parks on it; spinner/countdown rewrite that line in place with `\r\x1b[K`. Redraw does `\x1b[{n}F` (n = prev_lines-1) to jump to frame top. Trailing newline anywhere → cursor drift/scroll-shred in real terminals (invisible in piped captures — always test under `script -q /dev/null`).
- **Frame-shrink redraw**: when new frame is shorter than old (success frame ↔ 5-line error frame oscillates), the frame's LAST line must survive and stale rows below it must be cleared WITHOUT scrolling. Old code padded `\x1b[2K\r\n` then `\x1b[1F` — but each pad clears the row it starts on (wiping the just-written status line) and `1F` only returns `prev-n-1` rows short of the new bottom when shrinking by more than one line. Every shrink then desynced the next frame's `\x1b[{n}F` → stair-stepped repeated frames. Correct: from new bottom, `\x1b[1B`, clear-and-advance stale rows with `\x1b[2K\x1b[1B`, clear old bottom with `\x1b[2K`, then `\x1b[{prev-n}F` back to new bottom. Never write `\n` at the bottom row.
- **Transient TLS errors** (`tls connection init failed: unexpected end of file`): api.commandcode.ai drops handshakes intermittently. `api::get` retries up to 5× (1s base, 8s cap, fastrand jitter), 15s per-attempt timeout, transient matcher covers tls/connection/timeout/eof/handshake/certificate + HTTP 429/5xx (ureq error string `status code 429` / `status code 5`). 0.1.16; HTTP-status retry added after.
- **Update check is synchronous** (`update_check::check_sync`, called before the redraw loop): spawning a thread that eprintln!s asynchronously could garble the in-place redraw frame. One ≤5s block per day is fine.
- **stdout lock deadlock**: main thread holds `StdoutLock` for the whole loop; spinner threads must NOT write via `std::io::stdout()` — they block forever on the mutex and `join()` hangs, killing refreshes. Spinner writes to its own `/dev/tty` handle. This bit once; test watch mode under a pty (`script`) or it looks fine in captures.
- Spinner bug class: `start()` calls `stop()` first (safety), which sets stop_flag=true — MUST reset flag to false before spawning or thread exits instantly (zero frames, no error).
- Statusline: template engine in report_render.rs. Placeholders `{plan} {credits} {cap} {credits_bar} {5h_bar} {5h_pct} {5h_used} {5h_cap} {wk_bar} {wk_pct} {wk_used} {wk_cap}`. Unknown placeholders dropped, unclosed brace passes through, multi-line OK, `sl_colors=false` strips ANSI post-render, `sl_ascii=true` swaps ━╱ for #-.
- Burn rate: window spend ÷ time since window start (resetAt - dur). Warning only if projected cap-hit < reset AND ≥10% of window elapsed (flat-rate ETA unreliable early). Flat-rate assumption marked `ponytail:`. Same guard duplicated in src/lib.rs (Zed ext) + tui/src/render.rs.
- Time math: no chrono. All ISO/UTC date helpers live in dates.rs (`parse_iso_utc`, `civil_from_days`, `today_utc`, `day_shift`, `iso_hour_start`, `hour_label`). render.rs re-exports `parse_iso_utc`; reports.rs imports the rest. ISO treated as UTC (no offset parsing) — off by hours at most, acceptable.
- Config: `~/.config/cmd-usage/config.json` (XDG_CONFIG_HOME respected). interval clamp 1–86400, bar width 5–200. CLI flags override config. Config keys: `interval_secs, bar_width, statusline_template, statusline_colors, statusline_ascii`.
- `cmduse config set` keys: `interval=`, `width=`, `sl=`, `sl_colors=`, `sl_ascii=` (values with `=`, parse errors exit 2).

### Zed extension specifics

- Extensions CANNOT have custom panels/docks/timers. UI surfaces: slash commands (assistant panel), themes, languages, MCP/agent servers. The "dashboard" is a slash command output; the real UI is the cmduse CLI (terminal dock).
- Build target: wasm32-wasip1 (docs say wasip2; wasip1 works, CI builds it). `extension.wasm` at repo root is stale build output — gitignored, regenerated by Zed on dev-install.
- zed_extension_api 0.7.0: `Command::arg()` takes ownership (builder chain, no `cmd.arg()` reuse). `Output.status` is `Option<s32>`, not ExitStatus. HttpResponse has NO status field — non-200 surfaces as JSON parse error downstream.
- Slash command args: `requires_argument: false`; arg "plans" renders plans-only table.

### Publishing workflow (NEVER publish without explicit user go)

- crates.io: `cargo publish` in `tui/`. Requires clean git tree (commit first, including Cargo.lock — publish refuses dirty).
- Version discipline: 0.x line. Current: 0.1.17. History was re-shipped 0.1.3–0.1.8 from feature commits (temp git worktree at /tmp, version bumped, published); 0.2.0–0.4.0 yanked (crates.io can NEVER delete versions — yank only hides from resolution).
- crates.io API download URL 403s brew's UA — Homebrew formulas must use `https://static.crates.io/crates/<name>/<name>-<ver>.crate`.
- Homebrew tap: repo `JeffreyJYZ/homebrew-tap`, `Formula/cmduse.rb`. On every release: bump version, url, sha256 (`curl -sL https://static.crates.io/crates/cmd-usage/cmd-usage-<v>.crate | shasum -a 256`).
- README updates with EVERY user-facing change. Always.
- CI: GitHub Actions (ci.yml) — tui build/test/clippy `-D warnings`, Zed ext wasm build. Must pass before push lands on main.

### Testing

- `cargo test` in `tui/` (28 tests). Pure-function coverage: plan mapping, bars, money/compact, rel_time, ISO parse, elapsed %, window_line, renders (ANSI/plain/JSON), config round-trip with temp XDG dir, statusline templates, sparkline, redraw escape-sequence regression (main_tests.rs asserts exact control chars for grow/shrink/no-prev — shrink bug class is invisible in piped captures, assert the bytes not the screen).
- Watch-mode/spinner bugs only reproduce under a pty: `timeout 9 script -q /dev/null ./target/release/cmduse -i 3 | rg "fetching"` — piped captures hide TTY-gated code paths.
- Always `rg`, never grep. Tests updated in the same commit as the code they cover.

### Gotchas

- `git status` dirty blocks `cargo publish` — old staged files linger after checkouts; `git reset --hard HEAD` when unsure.
- history.jsonl in ~/.commandcode is CLI input history, not usage data.
- env::var("HOME") returns PathBuf-able string but `"...".into()` needs type annotation when chaining `.join()`.
- clippy in CI runs with `-D warnings`: `Result::map_err(|e| e)` identity, large Err closures (box or restructure to String), while_let_on_iterator, match_result_ok — fix, don't allow.
- ccusage-style local reports are complements, not replacements: local JSONL = per-model/per-project detail (CLI sessions only); API = account truth (all harnesses, but totals only).

# User Rules

- Always update AGENTS.md and README.md for each change you make.
- Always learn from the user when the user says explicit preferences, and note them in a "User Preferences" section in AGENTS.md.
- Always read a file before editing it
- For licenses, always fetch them from their source, never from memory.

# User Preferences

- TS formatting: Biome (biome.json at repo root, scope = opencode/**). Tab indent, width 4, lineWidth 100. `noExplicitAny` stays error (default) — don't relax. `noNonNullAssertion` off (scrape script needs it). Never add config options matching defaults.
- Use `rg`, never grep. pnpm/bun over npm. Homebrew for global apps.
- Root causes, not temporary fixes. No excess config options matching defaults.
- NEVER publish/push/release without explicit go.

## opencode plugin (opencode/)

- Package `opencode-command-code` (npm later; never publish without go). Build: `bun run build` (bun build + tsc d.ts), test: `bun test` (29 tests), typecheck: `bun run typecheck`.
- Model list: LIVE from `GET /provider/v1/models` (OpenAI shape, Bearer key) — endpoint is PUBLIC in docs (commandcode.ai/docs/provider), returns id/name/context_length. Claude models → Anthropic wire `/provider/v1/messages`, everything else → OpenAI wire `/provider/v1/chat/completions`; wrong lane = 400. So plugin registers TWO providers: `command-code-anthropic` (@ai-sdk/anthropic, claude ids only) + `command-code-openai` (@ai-sdk/openai-compatible, rest).
- Plan gating: extract-gating.ts scrapes installed CLI bundle (cli.mjs) → src/gating.ts (MODEL_CATEGORIES, PLAN_RULES, KNOWN_MODELS, MODEL_ALIASES, canonicalizeModelId). Regen: `bun run extract`. Anchors are minified var names (Fr/Ur/Sr/wr) — they shift between CLI releases.
- Gating evaluation (src/access.ts): purchased/free credits > 0 → all allowed; unknown plan → allow; category from exact id, else longest-prefix sibling ("claude-fable-5-1" → "claude-fable-5" → premium); else default-allow (API enforces real gate). No stem fallback — that mis-categorized muse-spark-1.3 as premium by matching the shorter muse-spark-1.1 sibling.
- Empirical model access reality (probed Sept 2026, GOAT plan): muse-spark-1.1 + gemini-3.5/3.6/3.5-lite/3.1-lite = 403 MODEL_NOT_IN_PLAN (not in CLI's category table; hardcoded in HARD_BLOCKED). gpt-5.6-luna sometimes returns transient 403 on first call (cold upstream key) then 200 — let it pass, do not block.
- Model id quirks: case-insensitive canonicalization; aliases (claude-opus-4-6→4-7); date suffixes stripped. Models.dev-style slash ids (z-ai/glm-5.3-flash) work as opencode model ids.
- Auth: plugin auth hook (type api) validates via /alpha/whoami → stored in ~/.local/share/opencode/auth.json. resolveKey order: CMD_API_KEY → opencode auth store (TODO: wire client.auth) → ~/.commandcode/auth.json.
- Model list stamped at config-hook time (startup); new models appear after restart (ponytail).
- Smoke test: add `"file:///tmp/opencode-smoke/src/index.ts"` to ~/.config/opencode/opencode.json plugin array; `opencode run --model command-code-openai/z-ai/glm-5.3-flash "hi" --print-logs` shows real errors (share subscriber ERROR line carries the cause).
- **Config-hook models DO surface in `opencode models` (v1.18.29) — but only if the plugin actually loads.** Debug chain that wasted an hour: (1) user's manual `command-code` provider entry in ~/.config/opencode/opencode.jsonc clobbered/merged with plugin output — its 2 models masked the bug; (2) file:// plugin entry with relative `./api` imports loads NOTHING silently (no log line) — plugin must be ONE self-contained module or live in `~/.config/opencode/plugins/` with deps resolvable from `~/.config/opencode/node_modules`; (3) bundled dist/index.js copied to `~/.config/opencode/plugins/command-code.js` works (bun bundle inlines src/, keeps @opencode-ai/plugin external).
- **Merge, don't clobber, user provider config**: config hook spreads `{...claudeDefs, ...userCc.models}` and preserves user `options` (user's jsonc entry has reasoning variants + modalities). User's 2 models must survive.
- **command-code-openai needs explicit `options.apiKey`**: opencode only injects stored auth for providers with their OWN /connect entry; the open lane shares command-code-anthropic's key → inject at config time via resolveKey (docs-sanctioned options.apiKey).
- Provider hook (`provider.models`, ≥1.14.49 path) present but unverified — config-hook path confirmed working; auth `loader` added following omniroute plugin precedent (dist/src/plugin.js in ~/.cache/opencode/packages/opencode-omniroute-auth@latest — best working reference for plugin provider/auth patterns).
- `/alpha/usage/summary` successRate is 0–100 (not 0–1) — render raw with `{:.0}%`.
