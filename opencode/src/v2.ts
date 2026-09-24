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
import { logEvent } from "./usagelog";

export const PLUGIN_ID = "command-code";
export const PROVIDER_BASE = "https://api.commandcode.ai/provider/v1";
/** Shared integration backing both lanes' credentials (/connect + env). */
export const INTEGRATION_ID = "command-code";
export const INTEGRATION_NAME = "Command Code";

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

/** The slice of the plugin context the credential resolver needs. */
type CredentialContext = {
	integration: {
		connection: {
			active(integrationID: string): Promise<unknown>;
			resolve(connection: unknown): Promise<unknown>;
		};
	};
};

/** Credential for spawning cmduse, host-first: a key stored by /connect lives
 * only in opencode's credential store, so ask the connection before falling
 * back to CMD_API_KEY / ~/.commandcode/auth.json. Resolved per call — a
 * /connect mid-session must be picked up. */
export async function credentialKey(ctx: CredentialContext): Promise<string | undefined> {
	try {
		const connection = await ctx.integration.connection.active(INTEGRATION_ID);
		if (connection) {
			const credential = (await ctx.integration.connection.resolve(connection)) as
				| { type: "key"; key: string }
				| { type: "oauth"; access: string }
				| undefined;
			if (credential?.type === "key") return credential.key;
			if (credential?.type === "oauth") return credential.access;
		}
	} catch {}
	try {
		return await resolveKey();
	} catch {
		return undefined;
	}
}

export const commandCodeV2 = Plugin.define({
	id: PLUGIN_ID,
	async setup(ctx) {
		// Credential sources, in host order of authority:
		//  1. the opencode connection for our integration (`/connect`, or the
		//     env method reading CMD_API_KEY on the server process)
		//  2. our own fallback: CMD_API_KEY here, then ~/.commandcode/auth.json
		//     (`cmd login`) — resolves users who never /connect-ed.
		// The host injects (1) into the provider runtime; (2) we inject as
		// settings.apiKey. Neither → activation "auto": availability follows the
		// integration connection, so a later /connect lights the provider up
		// without a plugin reload.
		let localKey: string | undefined;
		try {
			localKey = await resolveKey();
		} catch {}
		let hasConnection = false;
		try {
			hasConnection = (await ctx.integration.connection.active(INTEGRATION_ID)) !== undefined;
		} catch {}
		if (!localKey && !hasConnection) {
			console.warn(
				`[command-code] no API key found. Run /connect and choose "${INTEGRATION_NAME}", or \`cmd login\` (~/.commandcode/auth.json), or set CMD_API_KEY.`,
			);
		}

		// Same usage log as v1: subscribe to host events if the context offers
		// them, defensively so a shape change never breaks plugin setup.
		try {
			const events = (
				ctx as { event?: { subscribe?: (fn: (event: unknown) => void) => void } }
			).event;
			events?.subscribe?.((event) => logEvent(event));
		} catch {
			// no event surface on this host version
		}

		// /connect entry + env discovery for both lanes. Methods are additive
		// registrations on the shared integration id; the missing-record seed
		// behaves like the provider seed.
		await ctx.integration.transform((editor) => {
			editor.update(INTEGRATION_ID, (integration) => {
				if (integration.name === (integration.id as unknown as string)) {
					integration.name = INTEGRATION_NAME;
				}
			});
			editor.method.update({
				integrationID: INTEGRATION_ID,
				method: { type: "env", names: ["CMD_API_KEY"] },
			});
			editor.method.update({
				integrationID: INTEGRATION_ID,
				method: { type: "key", label: "Command Code API key" },
			});
		});

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
					// Availability follows the connection when there is one; with only
					// a local key we mark the provider enabled ourselves.
					provider.activation = hasConnection || localKey ? "enabled" : "auto";
					provider.settings = { ...provider.settings, baseURL: PROVIDER_BASE };
					if (!hasConnection && localKey && provider.settings.apiKey === undefined) {
						provider.settings.apiKey = localKey;
					}
					provider.integrationID ??= INTEGRATION_ID as never;
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
					const key = await credentialKey(ctx);
					return {
						content: await runCmduse(arg, { env: key ? { CMD_API_KEY: key } : undefined }),
					};
				},
			});
		});

		// Live model list: fetch + plan gating + lane split (same pipeline as
		// v1's provider.models hook), replace the static seed, then re-fetch on
		// an interval so models Command Code adds later show up without a
		// service restart (the competitor bakes its list at publish time).
		// The transform call itself marks the registry changed — no explicit
		// reload() needed. Resolves credentials per attempt, so a /connect
		// after load still fills the list.
		const controller = new AbortController();
		let refreshing = false;
		const refresh = async () => {
			if (refreshing || controller.signal.aborted) return;
			refreshing = true;
			try {
				const key = await credentialKey(ctx);
				if (!key) return;
				const split = await loadModels(key);
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
				console.warn("[command-code] live model list unavailable, keeping previous list:", e);
			} finally {
				refreshing = false;
			}
		};
		void refresh();
		// 30 min: comfortably past models.ts's 5-min cache, so every tick is a
		// real fetch. Guarded against overlap by `refreshing`.
		const timer = setInterval(() => void refresh(), 30 * 60 * 1000);
		return () => {
			controller.abort();
			clearInterval(timer);
		};
	},
});
