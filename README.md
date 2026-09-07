# Command Code tooling

Three tools around the [Command Code](https://commandcode.ai) API:

| Component | Path | What |
|---|---|---|
| `cmduse` | [`tui/`](tui/README.md) | Terminal dashboard: plan/credits/windows, account + local usage reports, statusline. Published as `cmd-usage` on crates.io (`brew install JeffreyJYZ/tap/cmduse`). |
| Zed extension | `src/` | `/cmd-usage` slash command in Zed's assistant panel (plan dashboard markdown). |
| opencode plugin | [`opencode/`](opencode/) | `opencode-command-code` — registers Command Code as an opencode provider (live model list, plan gating) plus a `cmd_usage` tool and `/cmd-usage` command. |

## opencode plugin

```sh
cd opencode && bun install && bun run build
cp dist/index.js ~/.config/opencode/plugins/command-code.js
```

Restart opencode. `/connect` → **Command Code** → paste your API key (or set `CMD_API_KEY`, or have `cmd login` done — the plugin reads that too).

What you get:

- **Two providers**: `command-code` (Claude models, Anthropic Messages wire) and `command-code-open` (everything else, OpenAI wire) — the API rejects the wrong wire per model, so the plugin splits them.
- **Live model list** from `GET /provider/v1/models`, filtered by your plan (go/goat = open models only, pro = no opus/fable, max/ultra/provider = everything, purchased credits = everything). Gating tables are extracted from the Command Code CLI bundle (`bun run extract` regenerates after CLI updates).
- **`cmd_usage` tool** — plan, credits, 5-hour/weekly windows, billing-period summary. `/cmd-usage` tells the agent to call it; `/cmd-usage plans` renders the plan comparison table.

Existing manual `command-code` provider config in `opencode.json(c)` is merged, not replaced — your model overrides and options win.

## Development

```sh
cd opencode
bun run build      # bundle + d.ts
bun test           # 26 tests
bun run typecheck  # tsc --noEmit
bun run extract    # regen gating tables from installed CLI bundle
```

Formatting: Biome (repo-root `biome.json`), tab indent width 4, lineWidth 100.

## License

MIT
