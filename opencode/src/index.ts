// Dual OpenCode entrypoint (one default export carries both APIs):
//   v1 (>=1.18.29) calls `server()` and consumes the returned hook map
//   v2 (>=2.0.0)   decodes `{ id, setup }` and calls `setup(context)`
// The two halves are independent implementations; v2 does not translate v1
// hooks. Both share ./cli.ts: usage rendering is delegated to the cmduse CLI
// (Rust cmduse-core) rather than reimplemented in TS.
import type { Plugin as PluginV1 } from "@opencode-ai/plugin";
import { z } from "zod";
import { API_BASE, whoami } from "./api";
import { runCmduse } from "./cli";
import { resolveKey } from "./key";
import { supportsImage } from "./modalities";
import { loadModels } from "./models";
import { commandCodeV2, PROVIDER_BASE } from "./v2";

/** Runtime `tool()` in @opencode-ai/plugin is the identity function — the
 * host builds tools from plain objects, so we define ours inline and import
 * only types from the plugin package. `zod` (a real schema) stays, since the
 * host reads `args` to build the tool's input shape. */
function tool<Args extends z.ZodRawShape>(input: {
	description: string;
	args: Args;
	execute: (args: { arg?: string }) => Promise<string>;
}): {
	description: string;
	args: Args;
	execute: (args: { arg?: string }) => Promise<string>;
} {
	return input;
}

type SdkModel = {
	id: string;
	providerID: string;
	api: { id: string; url: string; npm: string };
	name: string;
	capabilities: {
		temperature: boolean;
		reasoning: boolean;
		attachment: boolean;
		toolcall: boolean;
		input: { text: boolean; audio: boolean; image: boolean; video: boolean; pdf: boolean };
		output: { text: boolean; audio: boolean; image: boolean; video: boolean; pdf: boolean };
		/** A `{field}` object names the wire field reasoning must round-trip
		 * through (openai-compatible: "reasoning_content"). Without it,
		 * DeepSeek/GLM/Kimi reject the next request once an assistant turn
		 * carries no reasoning (compaction, model switch):
		 * "reasoning_content must be passed back". */
		interleaved: boolean | { field: string };
	};
	cost: { input: number; output: number; cache: { read: number; write: number } };
	limit: { context: number; output: number };
	status: "alpha" | "beta" | "deprecated" | "active";
	options: Record<string, unknown>;
	headers: Record<string, string>;
	release_date: string;
	/** opencode's config-hook model merge reads top-level `interleaved`;
	 * its provider.models hook reads `capabilities.interleaved`. Set both. */
	interleaved?: boolean | { field: string };
};

/** Every Command Code model shares these; hoisted so each record doesn't rebuild them. */
const MODEL_CAPABILITIES: Omit<SdkModel["capabilities"], "interleaved"> = {
	temperature: true,
	reasoning: true,
	attachment: false,
	toolcall: true,
	input: { text: true, audio: false, image: false, video: false, pdf: false },
	output: { text: true, audio: false, image: false, video: false, pdf: false },
};

/** Full ModelV2 shapes — the provider.models hook must return complete records. */
function toModelDefs(
	models: Array<{ id: string; name: string; contextLength: number }>,
	providerID: string,
	npm: string,
	interleaved: boolean | { field: string } = false,
): Record<string, SdkModel> {
	return Object.fromEntries(
		models.map((m) => [
			m.id,
			{
				id: m.id,
				providerID,
				api: { id: m.id, url: PROVIDER_BASE, npm },
				name: m.name,
				// The listing API has no capabilities, so vision comes from the
				// generated modalities table; text-only is the fallback.
				capabilities: {
					...MODEL_CAPABILITIES,
					interleaved,
					...(supportsImage(m.id)
						? { attachment: true, input: { ...MODEL_CAPABILITIES.input, image: true } }
						: {}),
				},
				cost: { input: 0, output: 0, cache: { read: 0, write: 0 } },
				limit: { context: m.contextLength || 128_000, output: 32_000 },
				status: "active" as const,
				options: {},
				headers: {},
				release_date: "",
				...(interleaved !== false ? { interleaved } : {}),
			},
		]),
	);
}

export const CommandCodePlugin: PluginV1 = async (_input) => {
	return {
		// /connect entry for the Claude lane. The open lane (`command-code`) is
		// intended to share the key via the user's existing manual config or via
		// options.apiKey injected at startup, so no second /connect entry is
		// needed.
		auth: {
			provider: "command-code-anthropic",
			loader: async (getAuth) => {
				const auth = await getAuth();
				if (auth?.type !== "api") {
					throw new Error(
						"No API key available. Please run '/connect' and choose Command Code (Anthropic).",
					);
				}
				return { apiKey: auth.key, baseURL: PROVIDER_BASE };
			},
			methods: [
				{
					type: "api",
					label: "Command Code API key (Claude / Anthropic)",
					prompts: [
						{
							type: "text",
							key: "apiKey",
							message: "Command Code API key (create at commandcode.ai/settings/keys)",
						},
					],
					async authorize(inputs) {
						const key = inputs?.apiKey?.trim();
						if (!key) return { type: "failed" };
						try {
							const me = await whoami(key);
							if (!me.success) return { type: "failed" };
							return { type: "success", key, provider: "command-code-anthropic" };
						} catch {
							return { type: "failed" };
						}
					},
				},
			],
		},

		// Register the Claude and OpenAI-compatible lanes. The two provider
		// ids this plugin owns (`command-code-anthropic`, `command-code-openai`)
		// are set here; user-defined entries for the same ids are preserved.
		config: async (cfg) => {
			cfg.provider ??= {};

			type ProviderModels = NonNullable<NonNullable<(typeof cfg)["provider"]>[string]["models"]>;
			const existing = (
				id: string,
			): {
				models?: ProviderModels;
				options?: Record<string, unknown>;
				npm?: string;
				name?: string;
			} =>
				(cfg.provider?.[id] ?? {}) as {
					models?: ProviderModels;
					options?: Record<string, unknown>;
					npm?: string;
					name?: string;
				};

			let split: Awaited<ReturnType<typeof loadModels>> | undefined;
			let openKey: string | undefined;
			// No key at config-hook time is expected for /connect-only users
			// (the key lives in the auth store, unreadable here) — the
			// provider.models hook below fills models with auth injected.
			try {
				openKey = await resolveKey();
			} catch {}
			// But a key we DID resolve followed by a fetch failure is a real
			// error (bad key / network) worth surfacing, not swallowing.
			if (openKey) {
				try {
					split = await loadModels(openKey);
				} catch (e) {
					console.warn("[command-code] model list unavailable:", e);
				}
			}

			// ponytail: config-hook models cover old paths; provider.models hook
			// (below) covers >=1.14.49. User-defined models always win the merge.
			// upgrade: drop config-hook registration once minimum supported opencode
			// is >=1.14.49 (provider.models hook supersedes it).
			// Verify 2026-09: `opencode models` (headless) lists command-code-openai/*
			// via the config hook, but zero command-code-anthropic/* — the anthropic
			// lane is auth-gated (its /connect entry has no stored key headlessly).
			// Confirm in the TUI after /connect, not via the CLI listing.
			const claudeDefs = split
				? toModelDefs(split.claude, "command-code-anthropic", "@ai-sdk/anthropic")
				: {};
			const openDefs = split
				? toModelDefs(
						split.open,
						"command-code-openai",
						"@ai-sdk/openai-compatible",
						{ field: "reasoning_content" },
					)
				: {};

			const userAnthropic = existing("command-code-anthropic");
			cfg.provider["command-code-anthropic"] = {
				npm: userAnthropic.npm ?? "@ai-sdk/anthropic",
				name: userAnthropic.name ?? "Command Code (Anthropic)",
				options: { baseURL: PROVIDER_BASE, ...userAnthropic.options },
				models: { ...claudeDefs, ...userAnthropic.models },
			};

			// open lane: opencode only injects stored auth for providers with
			// their own /connect entry; share the key via options.apiKey
			// resolved at startup (resolves against auth store + CLI auth.json).
			// ponytail: the plugin API allows one auth hook + one provider hook,
			// both bound to a single provider id, so a /connect-only user with no
			// ~/.commandcode/auth.json gets no open-lane models/key here. Fixing
			// that needs a second plugin entry (or an upstream multi-provider
			// hook); until then the open lane requires cmd login or options.apiKey.
			const userOpenai = existing("command-code-openai");
			cfg.provider["command-code-openai"] = {
				npm: userOpenai.npm ?? "@ai-sdk/openai-compatible",
				name: userOpenai.name ?? "Command Code (OpenAI)",
				options: {
					baseURL: PROVIDER_BASE,
					...(openKey ? { apiKey: openKey } : {}),
					...userOpenai.options,
				},
				models: { ...openDefs, ...userOpenai.models },
			};

			// /cmd-usage command -> agent calls the cmd_usage tool
			cfg.command ??= {};
			cfg.command["cmd-usage"] = {
				description: "Show Command Code plan, credits, and usage windows",
				template:
					"Call the cmd_usage tool and present its markdown output verbatim to the user. If the tool errors, tell the user to run /connect (Command Code (Anthropic)) and retry. $ARGUMENTS",
			};
		},

		// opencode >=1.14.49 resolves the model list here, with auth injected.
		provider: {
			id: "command-code-anthropic",
			models: async (_provider, ctx) => {
				const key = await resolveKey(async () => {
					const a = ctx.auth;
					return a?.type === "api" && a.key ? { key: a.key } : undefined;
				});
				const split = await loadModels(key);
				return toModelDefs(split.claude, "command-code-anthropic", "@ai-sdk/anthropic");
			},
		},

		tool: {
			cmd_usage: tool({
				description:
					"Fetch live Command Code plan/usage: plan name, monthly credits, 5-hour & weekly windows, billing-period summary. Pass arg=plans for the plan comparison table only.",
				args: {
					arg: z
						.string()
						.optional()
						.describe("Optional: 'plans' for the plan table only, or extra cmduse flags"),
				},
				// Rendering lives in the cmduse binary (Rust core); this is a thin
				// spawn wrapper. Piped stdout is plain text (colors auto-off).
				async execute(args) {
					const key = await resolveKey();
					return runCmduse(args.arg ?? "", { env: { CMD_API_KEY: key } });
				},
			}),
		},
	};
};

export default {
	...commandCodeV2,
	/** v1 entrypoint (opencode >=1.18.29 object entrypoints). */
	server: CommandCodePlugin,
};
