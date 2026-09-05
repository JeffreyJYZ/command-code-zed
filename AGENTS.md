# AGENTS.md

Command Code usage tooling: a Zed extension (WASM slash command) and `cmduse`, a standalone Rust CLI that renders Command Code plan/usage data in the terminal.

## Layout

```
extension.toml      Zed extension manifest (schema_version 1)
src/lib.rs          Zed extension: /cmd-usage slash command, assistant-panel markdown output
tui/                cmduse CLI crate (published as `cmd-usage` on crates.io, bin = `cmduse`)
  src/main.rs       entry: arg dispatch, watch loop (in-place redraw), statusline cmd
  src/cli.rs        arg parsing (Args, SubCmd, ConfigSet), usage text
  src/api.rs        API client: auth key read, GET helper, endpoint structs
  src/snapshot.rs   fetch orchestration: parallel endpoints, spinner thread
  src/render.rs     dashboard rendering (ANSI), bars, rel_time, ISO parsing, burn rate, sparkline
  src/reports.rs    local JSONL parser + account-wide daily/hourly (cumulative-diff), date math
  src/report_render.rs  statusline template engine + report tables (local/account/hourly/model/session)
  src/config.rs     config load/save, validation, XDG path
  src/render_tests.rs / config_tests.rs / cli_tests.rs   unit tests
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
- **stdout lock deadlock**: main thread holds `StdoutLock` for the whole loop; spinner threads must NOT write via `std::io::stdout()` — they block forever on the mutex and `join()` hangs, killing refreshes. Spinner writes to its own `/dev/tty` handle. This bit once; test watch mode under a pty (`script`) or it looks fine in captures.
- Spinner bug class: `start()` calls `stop()` first (safety), which sets stop_flag=true — MUST reset flag to false before spawning or thread exits instantly (zero frames, no error).
- Statusline: template engine in report_render.rs. Placeholders `{plan} {credits} {cap} {credits_bar} {5h_bar} {5h_pct} {5h_used} {5h_cap} {wk_bar} {wk_pct} {wk_used} {wk_cap}`. Unknown placeholders dropped, unclosed brace passes through, multi-line OK, `sl_colors=false` strips ANSI post-render, `sl_ascii=true` swaps ━╱ for #-.
- Burn rate: window spend ÷ time since window start (resetAt - dur). Warning only if projected cap-hit < reset. Flat-rate assumption marked `ponytail:`.
- Time math: no chrono. Howard Hinnant civil_from_days/days_from_civil inline in reports.rs + render.rs (`parse_iso_utc`). ISO treated as UTC (no offset parsing) — off by hours at most, acceptable.
- Config: `~/.config/cmd-usage/config.json` (XDG_CONFIG_HOME respected). interval clamp 1–86400, bar width 5–200. CLI flags override config. Config keys: `interval_secs, bar_width, statusline_template, statusline_colors, statusline_ascii`.
- `cmduse config set` keys: `interval=`, `width=`, `sl=`, `sl_colors=`, `sl_ascii=` (values with `=`, parse errors exit 2).

### Zed extension specifics

- Extensions CANNOT have custom panels/docks/timers. UI surfaces: slash commands (assistant panel), themes, languages, MCP/agent servers. The "dashboard" is a slash command output; the real UI is the cmduse CLI (terminal dock).
- Build target: wasm32-wasip1 (docs say wasip2; wasip1 works, CI builds it). `extension.wasm` at repo root is stale build output — gitignored, regenerated by Zed on dev-install.
- zed_extension_api 0.7.0: `Command::arg()` takes ownership (builder chain, no `cmd.arg()` reuse). `Output.status` is `Option<s32>`, not ExitStatus. HttpResponse has NO status field — non-200 surfaces as JSON parse error downstream.
- Slash command args: `requires_argument: false`; arg "plans" renders plans-only table.

### Publishing workflow (NEVER publish without explicit user go)

- crates.io: `cargo publish` in `tui/`. Requires clean git tree (commit first, including Cargo.lock — publish refuses dirty).
- Version discipline: 0.x line. Current: 0.1.10. History was re-shipped 0.1.3–0.1.8 from feature commits (temp git worktree at /tmp, version bumped, published); 0.2.0–0.4.0 yanked (crates.io can NEVER delete versions — yank only hides from resolution).
- crates.io API download URL 403s brew's UA — Homebrew formulas must use `https://static.crates.io/crates/<name>/<name>-<ver>.crate`.
- Homebrew tap: repo `JeffreyJYZ/homebrew-tap`, `Formula/cmduse.rb`. On every release: bump version, url, sha256 (`curl -sL https://static.crates.io/crates/cmd-usage/cmd-usage-<v>.crate | shasum -a 256`).
- README updates with EVERY user-facing change. Always.
- CI: GitHub Actions (ci.yml) — tui build/test/clippy `-D warnings`, Zed ext wasm build. Must pass before push lands on main.

### Testing

- `cargo test` in `tui/` (22 tests). Pure-function coverage: plan mapping, bars, money/compact, rel_time, ISO parse, elapsed %, window_line, renders (ANSI/plain/JSON), config round-trip with temp XDG dir, statusline templates, sparkline.
- Watch-mode/spinner bugs only reproduce under a pty: `timeout 9 script -q /dev/null ./target/release/cmduse -i 3 | rg "fetching"` — piped captures hide TTY-gated code paths.
- Always `rg`, never grep. Tests updated in the same commit as the code they cover.

### Gotchas

- `git status` dirty blocks `cargo publish` — old staged files linger after checkouts; `git reset --hard HEAD` when unsure.
- history.jsonl in ~/.commandcode is CLI input history, not usage data.
- env::var("HOME") returns PathBuf-able string but `"...".into()` needs type annotation when chaining `.join()`.
- clippy in CI runs with `-D warnings`: `Result::map_err(|e| e)` identity, large Err closures (box or restructure to String), while_let_on_iterator, match_result_ok — fix, don't allow.
- ccusage-style local reports are complements, not replacements: local JSONL = per-model/per-project detail (CLI sessions only); API = account truth (all harnesses, but totals only).
