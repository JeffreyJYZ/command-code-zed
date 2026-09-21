// OpenCode TUI half of the plugin: a slash command that runs the cmduse CLI
// in the terminal process and shows the dashboard directly in the UI.
//
// Why a TUI half at all: the server-side command path can only post session
// messages — synthetic messages are model-visible but NOT rendered in the TUI,
// and a prompt-based command costs an LLM round-trip. This half runs
// client-side (same machine, same auth.json), spawns the binary, and renders
// the output natively: zero model calls.
//
// Plain TS on purpose: keymap + dialog APIs need no JSX, so the package needs
// no OpenTUI/solid peer dependencies for this feature. The spawn wrapper is
// shared with the server half (./cli.ts) — one implementation.
import { Plugin } from "@opencode/plugin/tui"
import { runCmduse } from "./cli"

export default Plugin.define({
	id: "command-code.tui",
	setup(context) {
		context.keymap.layer(() => ({
			mode: "global",
			priority: 10,
			commands: [
				{
					id: "command-code.usage",
					title: "Command Code usage",
					group: "Command Code",
					// `/usage` is the primary name; `/cmd-usage` kept as the alias
					// users know from the plugin docs.
					slash: { name: "usage", aliases: ["cmd-usage"], arguments: true },
					suggested: true,
					run: async (input) => {
						let text: string
						try {
							text = await runCmduse(input ?? "")
						} catch (e) {
							context.ui.toast.show({
								title: "cmd-usage failed",
								message: e instanceof Error ? e.message : String(e),
								variant: "error",
							})
							return
						}
						await context.ui.dialog.alert({
							title: "Command Code usage",
							message: text.trimEnd(),
						})
					},
				},
			],
		}))
	},
})
