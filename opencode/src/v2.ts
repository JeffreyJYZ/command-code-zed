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
import type { Plugin as PluginNs } from "@opencode/plugin";
import { runCmduse } from "./cli";
import { KNOWN_MODELS } from "./gating";
import { inputModalities, isReasoningModel, modelCost, reasoningVariants } from "./catalog";
import { createCommandCode } from "./provider";
import { setupTimingLine, writeStartupLine } from "./startupLog";
import { resolveKey } from "./key";
import { isClaude, loadModels, type CmdModel } from "./models";

export const PLUGIN_ID = "command-code";
export const PROVIDER_BASE = "https://api.commandcode.ai/provider/v1";
/** Shared integration backing both lanes' credentials (/connect + env). */
export const INTEGRATION_ID = "command-code";
export const INTEGRATION_NAME = "Command Code";

// The host's `ProviderDomain.transform` / `ToolDomain.transform` callbacks do
// not infer their editor parameter through the package's re-exports, so the
// editor types are derived from the context itself. Keeps the file free of
// deep `@opencode/plugin/promise/*` imports (not a public subpath).
type V2Context = PluginNs.Context;
type ProviderEditor = Parameters<Parameters<V2Context["provider"]["transform"]>[0]>[0];
type IntegrationEditor = Parameters<Parameters<V2Context["integration"]["transform"]>[0]>[0];

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

export const TOOL_DESCRIPTION =
	"Fetch live Command Code plan/usage: plan name, monthly credits, 5-hour & weekly windows, billing-period summary. Pass arg='plans' for the plan comparison table only, or extra cmduse flags (e.g. '--tz +05:30 daily').";

/** opencode cost entry: $/1M rates, or [] when the catalog has none. */
function costEntry(id: string): Array<{ input: number; output: number; cache: { read: number; write: number } }> {
	const cost = modelCost(id)
	return cost ? [{ input: cost.input, output: cost.output, cache: { read: cost.cacheRead, write: cost.cacheWrite } }] : []
}

/** API/known model → v2 Model.Info. Capabilities, variants and $/1M cost all
 * come from the generated catalog (the listing API publishes none of them);
 * only the open lane needs the reasoning round-trip field. */
export function toV2Model(m: CmdModel, lane: Lane): unknown {
	// Reasoning-effort variants (ctrl+t cycling) come from the generated catalog:
	// the CLI bundle is the only source that lists per-model effort levels. v2
	// carries the effort in the variant's `settings`, which the host projects
	// into the model's provider options; models with no effort list stay empty.
	const efforts = reasoningVariants(m.id);
	const variants = efforts
		? Object.entries(efforts).map(([id, settings]) => ({ id, settings }))
		: [];
	const info = {
		id: m.id,
		modelID: m.id,
		providerID: lane.id,
		name: m.name,
		// Modalities come from the generated table (the API has no capabilities);
		// unknown models fall back to text-only.
		capabilities: { tools: true, input: [...inputModalities(m.id)], output: ["text"] },
		variants,
		time: { released: 0 },
		cost: costEntry(m.id),
		status: "active",
		enabled: true,
		limit: { context: m.contextLength || 128_000, output: 32_000 },
		...(lane.reasoningField ? { compatibility: { reasoningField: lane.reasoningField } } : {}),
	};
	return info;
}

/** Instant static seed from gating.json's known model ids, split by wire lane.
 * Shows in /model immediately; the live fetch merges over it later. */
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

/**
 * Union by model id: the snapshot keeps every id it had, so a gated or partial
 * live response can never make a model vanish from the picker, while live
 * entries win field-by-field so context, rates, variants and modalities stay
 * fresh. Snapshot order is preserved; live-only models append in live order.
 */
export function mergeModels(snapshot: readonly unknown[], live: readonly unknown[]): unknown[] {
	const byId = new Map<string, unknown>()
	const add = (model: unknown) => {
		const id = (model as { id?: unknown })?.id
		if (typeof id === "string") byId.set(id, model)
	}
	for (const model of snapshot) add(model)
	for (const model of live) add(model)
	return [...byId.values()]
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

/**
 * opencode validates the plugin *by decoding* the default export against its
 * own `Plugin` interface (identity `define` on both sides), so the package is
 * only needed for types. Keeps the runtime dependency surface to opencode's
 * own rewrite list (@opentui/*, solid-js).
 */
export const commandCodeV2: PluginNs.Plugin = {
	id: PLUGIN_ID,
	async setup(ctx) {
		const t0 = Date.now();
		// 1. Registration first. Nothing here may wait on the network or the
		//    credential store: /model lists a provider as soon as its models
		//    exist, and the seed gives them instantly. `resolveKey` is a local
		//    env/file read; the integration connection lookup below can take
		//    seconds and only *upgrades* what the seed already provides.
		let localKey: string | undefined;
		let keyMs = 0;
		try {
			const keyStart = Date.now();
			localKey = await resolveKey();
			keyMs = Date.now() - keyStart;
		} catch {}

		// /connect entry + env discovery for both lanes. Methods are additive
		// registrations on the shared integration id; the missing-record seed
		// behaves like the provider seed.
		await ctx.integration.transform((editor: IntegrationEditor) => {
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

		// Hand the host our own AI SDK provider for both lanes: streaming, image
		// parts and error surfacing are ours instead of opencode's internal
		// `@opencode/ai/providers/*`. The hook fires per model with the merged
		// settings (apiKey, baseURL) the host resolved for the connection.
		const images = (modelId: string) => inputModalities(modelId).includes("image");
		for (const lane of LANES) {
			await ctx.aisdk.hook(
				"sdk",
				(event) => {
					const options = event.options ?? {};
					event.sdk = createCommandCode(
						{
							apiKey: typeof options.apiKey === "string" ? options.apiKey : undefined,
							baseURL: typeof options.baseURL === "string" ? options.baseURL : PROVIDER_BASE,
							headers: options.headers as Record<string, string> | undefined,
						},
						images,
					);
				},
				{ providerID: lane.id as never },
			);
		}

		const seed = staticSeedModels();
		await ctx.provider.transform((editor: ProviderEditor) => {
			for (const lane of LANES) {
				// Missing providers seed from Provider.Info.empty(id), so `update`
				// gap-fills exactly like v1's `??=` fills; user config wins because
				// a non-seed value is left untouched.
				editor.update(lane.id, (provider) => {
					// `id` is a branded string; cast so the seed-value check compares
					// plain strings.
					if (provider.name === (provider.id as unknown as string)) provider.name = lane.name;
					if (provider.package === "") provider.package = lane.pkg;
					// A local key is enough to enable now; an integration connection
					// (looked up below) upgrades the same field when it lands.
					provider.activation = localKey ? "enabled" : "auto";
					provider.settings = { ...provider.settings, baseURL: PROVIDER_BASE };
					if (localKey && provider.settings.apiKey === undefined) {
						provider.settings.apiKey = localKey;
					}
					provider.integrationID ??= INTEGRATION_ID as never;
				});
				editor.models.set(lane.id, seed[lane.id] as never);
			}
		});
		const registerMs = Date.now() - t0;

		// v2 tool: same cmd_usage the v1 half registers. The Promise API takes
		// JSON Schema input and returns `{ content }`, so the agent keeps the
		// tool on both hosts. Cast because the shared schema types declare an
		// Effect-returning execute while the promise host awaits a plain Promise
		// (the shape opencode-cmd-provider also ships).
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
				async execute(input: unknown) {
					const arg =
						typeof (input as { arg?: unknown })?.arg === "string"
							? (input as { arg: string }).arg
							: "";
					const key = await credentialKey(ctx);
					return {
						content: await runCmduse(arg, { env: key ? { CMD_API_KEY: key } : undefined }),
					};
				},
			} as never);
		});

		// 2. Credentials. The connection lookup can take seconds (keyring, token
		//    refresh) and only upgrades what registration already provided:
		//    availability, and the key the lanes share.
		let hasConnection = false;
		let connectionMs = 0;
		try {
			const connectionStart = Date.now();
			hasConnection = (await ctx.integration.connection.active(INTEGRATION_ID)) !== undefined;
			connectionMs = Date.now() - connectionStart;
		} catch {}
		if (hasConnection) {
			await ctx.provider.transform((editor: ProviderEditor) => {
				for (const lane of LANES) {
					editor.update(lane.id, (provider) => {
						provider.activation = "enabled";
					});
				}
			});
		}
		if (!localKey && !hasConnection) {
			console.warn(
				`[command-code] no API key found. Run /connect and choose "${INTEGRATION_NAME}", or \`cmd login\` (~/.commandcode/auth.json), or set CMD_API_KEY.`,
			);
		}

		// 3. Live model list: fetch + plan gating + lane split (same pipeline as
		// v1's provider.models hook), merged over the snapshot, then re-fetched on
		// an interval so models Command Code adds later show up without a service
		// restart. Merge, not replace: a gated or partial response must never make
		// a model vanish from the picker. Resolves credentials per attempt, so a
		// /connect after load still fills the list.
		const controller = new AbortController();
		let refreshing = false;
		const refresh = async () => {
			if (refreshing || controller.signal.aborted) return;
			refreshing = true;
			const refreshStart = Date.now();
			try {
				const key = await credentialKey(ctx);
				if (!key) return;
				const split = await loadModels(key);
				if (controller.signal.aborted) return;
				await ctx.provider.transform((editor: ProviderEditor) => {
					editor.models.set(
						"command-code-anthropic",
						mergeModels(seed["command-code-anthropic"], split.claude.map((m) => toV2Model(m, LANES[0]!))) as never,
					);
					editor.models.set(
						"command-code-openai",
						mergeModels(seed["command-code-openai"], split.open.map((m) => toV2Model(m, LANES[1]!))) as never,
					);
				});
				// console.log from the server process never reaches opencode's log
				// file, so the timings go to our own cache file instead.
				void writeStartupLine(
					setupTimingLine({
						key: keyMs,
						register: registerMs,
						connection: connectionMs,
						refresh: Date.now() - refreshStart,
						models: split.claude.length + split.open.length,
					}),
				);
			} catch (e) {
				console.warn("[command-code] live model list unavailable, keeping the snapshot:", e);
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
};
