# Command Code tooling

[![CI](https://github.com/JeffreyJYZ/command-code-zed/actions/workflows/ci.yml/badge.svg)](https://github.com/JeffreyJYZ/command-code-zed/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cmd-usage.svg)](https://crates.io/crates/cmd-usage)
[![npm](https://img.shields.io/npm/v/@jeffreyjyz/opencode-command-code.svg)](https://www.npmjs.com/package/@jeffreyjyz/opencode-command-code)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](#license)

Everything Command Code (commandcode.ai) usage: a terminal dashboard with a
built-in MCP server, and an opencode provider. One Cargo workspace shares the
plan table and window math via `cmduse-core`; the opencode plugin is a
separate TS package that registers the providers and delegates usage
rendering to the `cmduse` CLI.

## Components

| Component | Crate / dir | Install | Docs |
|---|---|---|---|
| `cmduse` CLI | `cli/` (crate `cmd-usage`, bin `cmduse`) | [crates.io](https://crates.io/crates/cmd-usage) · [brew](https://github.com/JeffreyJYZ/homebrew-tap) | **[cli/README.md](cli/README.md)** · [man page](cli/cmduse.1) |
| Shared core | `core/` (crate `cmduse-core`) | [crates.io](https://crates.io/crates/cmduse-core) | [docs.rs/cmduse-core](https://docs.rs/cmduse-core) · versioned on its own `1.x` line, not as a pair with the CLI |
| opencode plugin | `opencode/` (`@jeffreyjyz/opencode-command-code`) | [npm](https://www.npmjs.com/package/@jeffreyjyz/opencode-command-code) | [opencode/src/index.ts](opencode/src/index.ts) |

## Install the CLI

```sh
brew install JeffreyJYZ/tap/cmduse     # macOS (Homebrew)
cargo install cmd-usage                # any platform with Rust
```

Then run `cmduse` for the live dashboard, or `cmduse plans` / `cmduse models`.

`cmduse model [--days N | --since ISO] [--json]` reports local per-model usage; without a window it
is all-time, so pass one when you mean a billing period. `--json` includes `source` and `since`.
There is no account-side per-model endpoint (the API only exposes totals), so local logs are the
only per-model source and may miss other machines or harnesses.

### Development binary

`cargo build --bin cmdusedev` builds the same program under a different name, so a local build
never shadows the Homebrew-installed `cmduse`. Tools that shell out can target it via
`CMDUSE_BIN=/path/to/cmdusedev`.
Full usage, config, and statusline docs live in **[cli/README.md](cli/README.md)**.

## Build

```sh
cargo build                      # all Rust crates
cargo test                       # core + cli (host tests)
cargo fmt --all -- --check       # formatting (CI gate)
cargo clippy --all-targets -- -D warnings
cargo package -p cmduse-core --allow-dirty   # ships plans.json+gating.json
cd opencode && bun install && bun test       # conformance vectors too
bun run extract                              # regen core/gating.json (needs CLI)
```

Shared truth lives in `core/`: `plans.json` (plan table/caps), `gating.json`
(model categories + per-plan access), `conformance.json` (behavior vectors).
`core/build.rs` bakes plans/gating into Rust consts; the opencode plugin's
model-gating layer imports `gating.json` and asserts the gating subset of the
vectors (usage/window math is the Rust core's alone since plugin 0.2.0).

## opencode plugin

Providers (`command-code-anthropic`, `command-code-openai`), a live gated
model list, `/usage` (TUI slash command, alias `/cmd-usage`), and the
`cmd_usage` tool — for **both opencode v1 (≥1.18.29) and v2 (≥2.0.0)** from
one package.

Requires the `cmduse` CLI (usage windows/pace rendering live in the Rust
core — the plugin spawns it):

```sh
brew install JeffreyJYZ/tap/cmduse
```

Install (opencode v2 uses `plugins`; v1's singular `plugin` is auto-normalized):

```json
{
  "plugins": ["@jeffreyjyz/opencode-command-code"]
}
```

Auth, in host order: opencode's own connection — **`/connect` and pick
"Command Code"** (or the `CMD_API_KEY` env method) — then our fallback
`~/.commandcode/auth.json` from `cmd login`. With no credential at all the
providers stay `activation: "auto"`, so a later `/connect` lights them up
without a restart.

### Sidebar

Vision models (Claude, Gemini, GPT, Qwen, the DeepSeek `-vision-` ones) accept image attachments;
text-only models do not. The per-model list is generated from Command Code's own CLI table — the
listing API publishes no capabilities.

While a session uses a `command-code*` model, the session sidebar grows a **Command Code**
section (toggle with `ctrl+x b`):

- plan, price and monthly credits used
- 5-hour and weekly windows: used / cap, percent, reset countdown
- this period's requests and spend
- the active model: tier, monthly allowance, $/M rates (in/out, cache read), Intelligence, Tok/s
  (new in 0.2.5)
- the active model's own period usage — requests, plus spend when the harness records it
  (new in 0.2.9)

Usage comes from the `cmduse` CLI (polled every 30s); the model catalog comes from `mpc --json`,
cached for 6h — install it with `bun link` in the sibling `oc-cmd-compare` checkout, or the section
simply omits those rows. The model's own usage is read from opencode's message store
(`~/.local/share/opencode/opencode.db`, read-only); CommandCode is subscription-billed, so its
rows show requests only. Non-CommandCode models show nothing.

## MCP server

`cmduse mcp` runs an MCP stdio server (hand-rolled JSON-RPC, no extra deps)
exposing five tools: `usage` (dashboard), `plans` (comparison table),
`models` (live gated list), `daily`, and `hourly` — the same output the CLI
subcommands print, with auth and `~/.commandcode/auth.json` shared.

Zed (`~/.config/zed/settings.json`):

```json
{
  "context_servers": {
    "cmduse": { "source": "custom", "command": "cmduse", "args": ["mcp"] }
  }
}
```

Any other MCP host: run `cmduse mcp` as a stdio server. The opencode plugin
doesn't need it — it registers its own providers and spawns `cmduse` directly.

## Why the split

0.1.x duplicated pure logic across two Rust crates (e.g. the burn-rate pace
gate was patched in two files for one bug). The workspace moves all shared
math into `core/`; `cli/` keeps only its presentation + I/O. The Zed
extension (deprecated 0.6.9) was replaced by the built-in MCP server — one
integration serves every MCP-capable host instead of one hand-maintained
WASM product per editor.

## License

MIT — see [cli/LICENSE-MIT](cli/LICENSE-MIT), [core/LICENSE-MIT](core/LICENSE-MIT), and [opencode/LICENSE](opencode/LICENSE).
