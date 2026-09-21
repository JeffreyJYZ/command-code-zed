// OpenCode v2 half of the dual entrypoint (see ./index.ts for the loader
// contract). v2 calls `setup(ctx)` once at plugin load; every edit is a
// transform: a synchronous, replayable state edit the host re-applies onto a
// fresh value whenever registrations change.
//
// v1 hook → v2 destination used here:
//   `config` (providers + models) → `ctx.provider.transform`
//   `config` (command)            → `ctx.command.transform`
//   `tool` map                    → `ctx.tool.transform`
//   `auth` hook                   → none (v2 has no per-provider auth-method
//                                    hook for plugin-defined providers; the
//                                    key comes from resolveKey() and is
//                                    injected as `settings.apiKey`)
//
// Usage rendering is NOT reimplemented here: the /cmd-usage command and the
// cmd_usage tool both spawn the cmduse CLI (Rust cmduse-core), which owns all
// window math and formatting (see ./cli.ts).
import { Plugin } from "@opencode/plugin";
import { runCmduse } from "./cli";
import { KNOWN_MODELS } from "./gating";
import { resolveKey } from "./key";
import { isClaude, loadModels, type CmdModel } from "./models";

export const PLUGIN_ID = "command-code";
export const PROVIDER_BASE = "https://api.commandcode.ai/provider/v1";

type Lane = {
	id: "command-code-anthropic" | "command-code-openai";
	name: string;
	/** v2 runtime package (first-party, distinct from the v1 `npm` value). */
	pkg: string;
	/** Wire field reasoning must round-trip through on the openai-compatible
	 * lane; v2 spells the old `interleaved: {field}` as `compatibility`. */
	reasoningField?: string;
};
export type { Lane };

const LANES: Lane[] = [
	{ id: "command-code-anthropic", name: "Command Code (Anthropic)", pkg: "@opencode/ai/providers/anthropic" },
	{
		id: "command-code-openai",
		name: "Command Code (OpenAI)",
		pkg: "@opencode/ai/providers/openai-compatible",
		reasoningField: "reasoning_content",
	},
];

const TOOL_DESCRIPTION =
	"Fetch live Command Code plan/usage: plan name, monthly credits, 5-hour & weekly windows, billing-period summary. Pass arg='plans' for the plan comparison table only, or extra cmduse flags (e.g. '--tz +05:30 daily').";

/** API/known model → v2 Model.Info. Every Command Code model shares
 * capabilities and pricing (subscription credits, not per-token); only the
 * open lane needs the reasoning round-trip field. */
export function toV2Model(m: CmdModel, lane: Lane): unknown {
	const info = {
		id: m.id,
		modelID: m.id,
		providerID: lane.id,
		name: m.name,
		capabilities: { tools: true, input: ["text"], output: ["text"] },
		variants: [],
		time: { released: 0 },
		cost: [],
		status: "active",
		enabled: true,
		limit: { context: m.contextLength || 128_000, output: 32_000 },
		...(lane.reasoningField ? { compatibility: { reasoningField: lane.reasoningField } } : {}),
	};
	return info;
}

/** Instant static seed from gating.json's known model ids, split by wire lane.
 * Replaced by the gated live list once the background fetch lands; also the
 * fallback shown when the live fetch fails. */
export function staticSeedModels(): Record<Lane["id"], unknown[]> {
	const seed: Record<Lane["id"], unknown[]> = {
		"command-code-anthropic": [],
		"command-code-openai": [],
	};
	for (const lane of LANES) {
		const models = KNOWN_MODELS.filter((id) => isClaude(id) === (lane.reasoningField === undefined)).map(
			(id) => toV2Model({ id, name: id, contextLength: 0 }, lane),
		);
		seed[lane.id] = models;
	}
	return seed;
}

export const commandCodeV2 = Plugin.define({
	id: PLUGIN_ID,
	async setup(ctx) {
		// Resolved once per load: env override, then ~/.commandcode/auth.json
		// (same order as cmduse). No key = providers registered disabled + warn
		// (a degraded registration beats a plugin that will not load).
		let key: string | undefined;
		try {
			key = await resolveKey();
		} catch {}
		if (!key) {
			console.warn(
				"[command-code] no API key found — providers registered disabled. `cmd login` (~/.commandcode/auth.json), CMD_API_KEY, or providers.command-code-*.settings.apiKey in opencode.jsonc, then restart.",
			);
		}

		const seed = staticSeedModels();
		await ctx.provider.transform((editor) => {
			for (const lane of LANES) {
				// Missing providers seed from Provider.Info.empty(id), so `update`
				// gap-fills exactly like v1's `??=` fills; user config wins because
				// a non-seed value is left untouched.
				editor.update(lane.id, (provider) => {
					// `id` is a branded string; cast so the seed-value check compares
					// plain strings.
					if (provider.name === (provider.id as unknown as string)) provider.name = lane.name;
					if (provider.package === "") provider.package = lane.pkg;
					provider.activation = key ? "enabled" : "disabled";
					provider.settings = { ...provider.settings, baseURL: PROVIDER_BASE };
					if (key && provider.settings.apiKey === undefined) provider.settings.apiKey = key;
				});
				editor.models.set(lane.id, seed[lane.id] as never);
			}
		});

		// NOTE: no server-side /cmd-usage command here — synthetic messages are
		// model-visible but not rendered in the TUI, and a prompt-based command
		// costs an LLM round-trip. The user-facing slash command lives in the
		// TUI half (src/tui.ts, dist/tui.js via the package ./tui export).

		await ctx.tool.transform((editor) => {
			editor.add({
				name: "cmd_usage",
				description: TOOL_DESCRIPTION,
				input: {
					type: "object",
					properties: {
						arg: {
							type: "string",
							description:
								"Optional: 'plans' for the plan table only, or extra cmduse flags (e.g. '--tz +05:30 daily')",
						},
					},
					additionalProperties: false,
				},
				async execute(input) {
					const arg = typeof (input as { arg?: unknown })?.arg === "string" ? (input as { arg: string }).arg : "";
					return {
						content: await runCmduse(arg, { env: key ? { CMD_API_KEY: key } : undefined }),
					};
				},
			});
		});

		if (!key) return;

		// Live model list in the background: fetch + plan gating + lane split
		// (same pipeline as v1's provider.models hook), then replace the static
		// seed. The transform call itself marks the registry changed — no
		// explicit reload() needed.
		const controller = new AbortController();
		void (async () => {
			try {
				const split = await loadModels(key!);
				if (controller.signal.aborted) return;
				await ctx.provider.transform((editor) => {
					editor.models.set(
						"command-code-anthropic",
						split.claude.map((m) => toV2Model(m, LANES[0]!)) as never,
					);
					editor.models.set(
						"command-code-openai",
						split.open.map((m) => toV2Model(m, LANES[1]!)) as never,
					);
				});
			} catch (e) {
				console.warn("[command-code] live model list unavailable, keeping static seed:", e);
			}
		})();
		return () => controller.abort();
	},
});
