# Command Code tooling

[![CI](https://github.com/JeffreyJYZ/command-code-zed/actions/workflows/ci.yml/badge.svg)](https://github.com/JeffreyJYZ/command-code-zed/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cmd-usage.svg)](https://crates.io/crates/cmd-usage)
[![npm](https://img.shields.io/npm/v/@jeffreyjyz/opencode-command-code.svg)](https://www.npmjs.com/package/@jeffreyjyz/opencode-command-code)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](#license)

Everything Command Code (commandcode.ai) usage: a terminal dashboard, a Zed
slash command, and an opencode provider. One Cargo workspace shares the plan
table and window math via `cmduse-core`; the opencode plugin is a separate TS
package that imports the same JSON.

## Components

| Component | Crate / dir | Install | Docs |
|---|---|---|---|
| `cmduse` CLI | `cli/` (crate `cmd-usage`, bin `cmduse`) | [crates.io](https://crates.io/crates/cmd-usage) · [brew](https://github.com/JeffreyJYZ/homebrew-tap) | **[cli/README.md](cli/README.md)** · [man page](cli/cmduse.1) |
| Zed extension | `zed-ext/` (crate `command-code-usage`) | dev-install (WASM, no release channel) | [zed-ext/src/lib.rs](zed-ext/src/lib.rs) |
| Shared core | `core/` (crate `cmduse-core`) | [crates.io](https://crates.io/crates/cmduse-core) | [docs.rs/cmduse-core](https://docs.rs/cmduse-core) |
| opencode plugin | `opencode/` (`@jeffreyjyz/opencode-command-code`) | [npm](https://www.npmjs.com/package/@jeffreyjyz/opencode-command-code) | [opencode/src/index.ts](opencode/src/index.ts) |

## Install the CLI

```sh
brew install JeffreyJYZ/tap/cmduse     # macOS (Homebrew)
cargo install cmd-usage                # any platform with Rust
```

Then run `cmduse` for the live dashboard, or `cmduse plans` / `cmduse models`.
Full usage, config, and statusline docs live in **[cli/README.md](cli/README.md)**.

## Build

```sh
cargo build                      # all Rust crates
cargo test                       # core + cli + zed-ext (host tests)
cargo fmt --all -- --check       # formatting (CI gate)
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

## License

MIT — see [cli/LICENSE-MIT](cli/LICENSE-MIT) and [opencode/LICENSE](opencode/LICENSE).
