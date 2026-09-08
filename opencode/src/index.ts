import type { Plugin } from "@opencode-ai/plugin";
import { tool } from "@opencode-ai/plugin";
import { whoami } from "./api";
import { resolveKey } from "./key";
import { loadModels } from "./models";
import { plansTable, renderUsage } from "./usage";

const PROVIDER_BASE = "https://api.commandcode.ai/provider/v1";

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
		interleaved: boolean;
	};
	cost: { input: number; output: number; cache: { read: number; write: number } };
	limit: { context: number; output: number };
	status: "alpha" | "beta" | "deprecated" | "active";
	options: Record<string, unknown>;
	headers: Record<string, string>;
	release_date: string;
};

/** Full ModelV2 shapes — the provider.models hook must return complete records. */
function toModelDefs(
	models: Array<{ id: string; name: string; contextLength: number }>,
	providerID: string,
	npm: string,
): Record<string, SdkModel> {
	return Object.fromEntries(
		models.map((m) => [
			m.id,
			{
				id: m.id,
				providerID,
				api: { id: m.id, url: PROVIDER_BASE, npm },
				name: m.name,
				capabilities: {
					temperature: true,
					reasoning: true,
					attachment: false,
					toolcall: true,
					interleaved: false,
					input: { text: true, audio: false, image: false, video: false, pdf: false },
					output: { text: true, audio: false, image: false, video: false, pdf: false },
				},
				cost: { input: 0, output: 0, cache: { read: 0, write: 0 } },
				limit: { context: m.contextLength || 128_000, output: 32_000 },
				status: "active" as const,
				options: {},
				headers: {},
				release_date: "",
			},
		]),
	);
}

export const CommandCodePlugin: Plugin = async (_input) => {
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
			): { models?: ProviderModels; options?: Record<string, unknown> } =>
				(cfg.provider?.[id] ?? {}) as {
					models?: ProviderModels;
					options?: Record<string, unknown>;
				};

			let split: Awaited<ReturnType<typeof loadModels>> | undefined;
			let openKey: string | undefined;
			try {
				openKey = await resolveKey();
				split = await loadModels(openKey);
			} catch {}

			// ponytail: config-hook models cover old paths; provider.models hook
			// (below) covers >=1.14.49. User-defined models always win the merge.
			const claudeDefs = split
				? toModelDefs(split.claude, "command-code-anthropic", "@ai-sdk/anthropic")
				: {};
			const openDefs = split
				? toModelDefs(split.open, "command-code-openai", "@ai-sdk/openai-compatible")
				: {};

			const userAnthropic = existing("command-code-anthropic");
			cfg.provider["command-code-anthropic"] = {
				npm: userAnthropic.options ? undefined : "@ai-sdk/anthropic",
				name: "Command Code (Anthropic)",
				options: { baseURL: PROVIDER_BASE, ...userAnthropic.options },
				models: { ...claudeDefs, ...userAnthropic.models },
			};

			// open lane: opencode only injects stored auth for providers with
			// their own /connect entry; share the key via options.apiKey
			// resolved at startup (resolves against auth store + CLI auth.json).
			const userOpenai = existing("command-code-openai");
			cfg.provider["command-code-openai"] = {
				npm: userOpenai.options ? undefined : "@ai-sdk/openai-compatible",
				name: userOpenai.options ? undefined : "Command Code (OpenAI)",
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
					arg: tool.schema
						.string()
						.optional()
						.describe("Optional: 'plans' for the plan table only"),
				},
				async execute(args) {
					if (args.arg === "plans") return plansTable("");
					const key = await resolveKey();
					return renderUsage(key);
				},
			}),
		},
	};
};

export default CommandCodePlugin;
