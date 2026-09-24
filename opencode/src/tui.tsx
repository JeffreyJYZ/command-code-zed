/** @jsxImportSource @opentui/solid */
// OpenCode TUI half: a "Command Code" session-sidebar section (plan, rolling
// windows, period totals, plus the active model's allowance, rates and
// benchmarks), and — on v2 hosts — the /cmd-usage slash command that prints the
// full cmduse dashboard in a dialog.
//
// Two hosts, two TUI contracts, one module (same shape cmd-provider uses):
//   v1  `tui(api)`             — `api.slots.register({ slots: { sidebar_content } })`
//   v2  `setup(context)`       — `context.ui.slot({ append: "sidebar.content" })`
// Data comes from two read-only spawns: cmduse for usage (polled) and mpc for
// the per-model catalog (disk-cached; it scrapes docs).
import type { RGBA } from "@opentui/core"
import type { JSX } from "@opentui/solid"
import { Plugin } from "@opencode/plugin/tui"
import { For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js"
import { runCmduse } from "./cli"
import { loadMeta, loadUsage } from "./sidebar/data"
import {
	type ModelMeta,
	type ModelUsage,
	type SidebarRow,
	modelKey,
	modelRows,
	tierFor,
	usageRows,
} from "./sidebar/rows"
import { loadModelUsage, periodStart } from "./sidebar/usageDb"

// Plugin id is a stable contract (test/tui.test.ts pins it); the slot id is separate.
const ID = "command-code.tui"
const POLL_MS = 30_000

/** Shared panel: bold title, one line per row. Renders nothing when empty. */
function Panel(props: { rows: () => SidebarRow[]; text: () => RGBA; muted: () => RGBA }) {
	return (
		<Show when={props.rows().length > 0}>
			<box>
				<text fg={props.text()}>
					<b>Command Code</b>
				</text>
				<For each={props.rows()}>
					{(row) => (
						<text fg={props.muted()}>{row[1] ? `${row[0]}: ${row[1]}` : row[0]}</text>
					)}
				</For>
			</box>
		</Show>
	)
}

/** Usage (polled) + model meta (cached), reduced to sidebar rows. */
function useRows(activeModelId: () => string | undefined, active: () => boolean) {
	const [usage, setUsage] = createSignal<ReturnType<typeof usageRows>>([])
	const [meta, setMeta] = createSignal<Map<string, ModelMeta>>(new Map())
	const [modelUsage, setModelUsage] = createSignal<ModelUsage | undefined>()

	void loadMeta()
		.then(setMeta)
		.catch(() => {})
	const refresh = async () => {
		try {
			const snapshot = await loadUsage()
			setUsage(usageRows(snapshot))
			// opencode's own store carries the per-model half cmduse lacks.
			const id = activeModelId()
			setModelUsage(
				id ? (loadModelUsage(id, periodStart(snapshot.periodEnd)) ?? undefined) : undefined,
			)
		} catch {
			// cmduse missing/offline: keep the last snapshot
		}
	}
	// Poll only while this session is on one of our models. The panel returns no
	// rows otherwise, but an unconditional poll still spawned cmduse for every
	// session on every provider — and cmduse's spinner writes to /dev/tty.
	createEffect(() => {
		if (!active()) return
		void refresh()
		const timer = setInterval(() => void refresh(), POLL_MS)
		onCleanup(() => clearInterval(timer))
	})

	return createMemo(() => {
		if (!active()) return []
		const id = activeModelId()
		const found = id ? (meta().get(modelKey(id)) ?? meta().get(id)) : undefined
		// gating keys are model ids, not display names, so tier comes from the
		// session's id rather than the catalog row.
		const model = found ? { ...found, tier: found.tier ?? (id ? tierFor(id) : undefined) } : undefined
		return [...usage(), ...modelRows(model, modelUsage())]
	})
}

const isOurs = (providerID: string | undefined): boolean =>
	Boolean(providerID?.startsWith("command-code"))

// ---- v1 host ---------------------------------------------------------------

interface V1Api {
	slots: {
		register: (config: {
			order?: number
			slots: {
				sidebar_content: (
					ctx: unknown,
					props: { session_id: string },
				) => unknown
			}
		}) => void
	}
	state: {
		session: {
			get: (id: string) => { model?: { providerID: string; id: string } } | undefined
		}
	}
	theme: { current: { text: RGBA; textMuted: RGBA } }
}

function PanelV1(props: { api: V1Api; sessionID: string }) {
	const current = () => props.api.state.session.get(props.sessionID)?.model
	const rows = useRows(
		() => current()?.id,
		() => isOurs(current()?.providerID),
	)
	return (
		<Panel
			rows={rows}
			text={() => props.api.theme.current.text}
			muted={() => props.api.theme.current.textMuted}
		/>
	)
}

/** v1 half — snake_case slot map registered through `api.slots`. */
export const tui = async (api: V1Api): Promise<void> => {
	api.slots.register({
		order: 200,
		slots: {
			sidebar_content(_ctx, props) {
				return <PanelV1 api={api} sessionID={props.session_id} />
			},
		},
	})
}

// ---- v2 host ---------------------------------------------------------------

function PanelV2(props: {
	ctx: Parameters<Parameters<typeof Plugin.define>[0]["setup"]>[0]
	sessionID: string
}) {
	const { ctx } = props
	const current = () => ctx.data.session.get(props.sessionID)?.model
	const rows = useRows(
		() => current()?.id,
		() => isOurs(current()?.providerID),
	)
	return (
		<Panel
			rows={rows}
			text={() => ctx.theme.text.default}
			muted={() => ctx.theme.text.subdued}
		/>
	)
}

export const commandCodeTui = Plugin.define({
	id: ID,
	setup(ctx) {
		ctx.ui.slot({
			append: "sidebar.content",
			render: (input) => <PanelV2 ctx={ctx} sessionID={input.sessionID} />,
		})
		// /cmd-usage prints the full cmduse dashboard in a dialog. keymap.layer()
		// must run inside a render, so mount a no-op and register from there.
		// Return `void` like opencode's own /btw claim: a `null` render leaves an
		// empty box that paints a stray line in the prompt area.
		ctx.ui.slot({
			append: "app",
			render: (() => {
				ctx.keymap.layer(() => ({
					mode: "global",
					priority: 10,
					commands: [
						{
							id: "command-code.cmd-usage",
							title: "Command Code usage",
							group: "Command Code",
							slash: { name: "cmd-usage", arguments: true },
							enabled: () => true,
							suggested: true,
							run: async (input) => {
								let text: string
								try {
									text = await runCmduse(input ?? "")
								} catch (error) {
									ctx.ui.toast.show({
										title: "cmd-usage failed",
										message: error instanceof Error ? error.message : String(error),
										variant: "error",
									})
									return
								}
								await ctx.ui.dialog.alert({
									title: "Command Code usage",
									message: text.trimEnd(),
								})
							},
						},
					],
					bindings: ["command-code.cmd-usage"],
				}))
			}) as unknown as () => JSX.Element,
		})
	},
})

export default { ...commandCodeTui, tui }
