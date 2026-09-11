# Command Code tooling

Cargo workspace. One shared logic crate (`cmduse-core`) powering two thin
UIs; the opencode plugin is a separate TS package.

| Component | Crate/dir | What |
|---|---|---|
| `cmduse` CLI | `cli/` (`cmd-usage`, bin `cmduse`) | Terminal dashboard: plan/credits/windows, watch mode, reports, statusline. Published on crates.io / Homebrew. |
| Zed extension | `zed-ext/` (`command-code-usage`) | `/cmd-usage` slash command in Zed assistant panel (markdown dashboard). WASM build. |
| Shared core | `core/` (`cmduse-core`) | Plan table, dates/ISO helpers, window/pace math, API wire DTOs, money/compact/pct/rel_time formatting. Single source — edit here, both UIs pick it up. |
| opencode plugin | `opencode/` | `@jeffreyjyz/opencode-command-code` — Command Code as an opencode provider (live model list, plan gating). Imports `core/plans.json` + `core/gating.json` directly; its npm version is intentionally independent of the Rust workspace version. |

## Build

```sh
cargo build                      # all Rust crates
cargo test                       # core + cli + zed-ext (host tests)
cargo clippy --all-targets -- -D warnings
cargo build -p command-code-usage --target wasm32-wasip1 --release   # Zed ext
cargo package -p cmduse-core --allow-dirty   # ships plans.json+gating.json
cd opencode && bun install && bun test       # conformance vectors too
```

Shared truth lives in `core/`: `plans.json` (plan table/caps), `gating.json`
(model categories + per-plan access), `conformance.json` (behavior vectors).
`core/build.rs` bakes plans/gating into Rust consts; `opencode` imports the
same JSON, and both sides assert the vectors so the ports can't drift.

## Why the split

0.1.x duplicated pure logic across two Rust crates (e.g. the burn-rate pace
gate was patched in two files for one bug). The workspace moves all shared
math into `core/`; `cli/` and `zed-ext/` keep only their presentation + I/O.
