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
		// /connect entry: user pastes their Command Code API key.
		auth: {
			provider: "command-code",
			loader: async (getAuth) => {
				const auth = await getAuth();
				if (auth?.type !== "api") {
					throw new Error("No API key available. Please run '/connect' and choose Command Code.");
				}
				return { apiKey: auth.key, baseURL: PROVIDER_BASE };
			},
			methods: [
				{
					type: "api",
					label: "Command Code API key",
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
							return { type: "success", key, provider: "command-code" };
						} catch {
							return { type: "failed" };
						}
					},
				},
			],
		},

		// Register both wire lanes as config-time providers. Merges with any
		// user-defined provider config (e.g. a manual "command-code" entry in
		// opencode.jsonc) instead of clobbering it. Models fetched live with
		// whatever key resolveKey finds (CMD_API_KEY or ~/.commandcode/auth.json).
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
			try {
				split = await loadModels(await resolveKey());
			} catch {}

			// ponytail: config-hook models cover old paths; provider.models hook
			// (below) covers >=1.14.49. User-defined models always win the merge.
			const claudeDefs = split
				? toModelDefs(split.claude, "command-code", "@ai-sdk/anthropic")
				: {};
			const openDefs = split
				? toModelDefs(split.open, "command-code-open", "@ai-sdk/openai-compatible")
				: {};

			const userCc = existing("command-code");
			cfg.provider["command-code"] = {
				npm: userCc.options ? undefined : "@ai-sdk/anthropic",
				name: "Command Code",
				options: { baseURL: PROVIDER_BASE, ...userCc.options },
				models: { ...claudeDefs, ...userCc.models },
			};
			const userOpen = existing("command-code-open");
			// opencode only injects stored auth for providers with their own /connect
			// entry; command-code-open shares the command-code key, so inject it here.
			let openKey: string | undefined;
			try {
				openKey = await resolveKey();
			} catch {}
			cfg.provider["command-code-open"] = {
				npm: "@ai-sdk/openai-compatible",
				name: userOpen.options ? undefined : "Command Code (Open)",
				options: {
					baseURL: PROVIDER_BASE,
					...(openKey ? { apiKey: openKey } : {}),
					...userOpen.options,
				},
				models: { ...openDefs, ...userOpen.models },
			};

			// /cmd-usage command -> agent calls the cmd_usage tool
			cfg.command ??= {};
			cfg.command["cmd-usage"] = {
				description: "Show Command Code plan, credits, and usage windows",
				template:
					"Call the cmd_usage tool and present its markdown output verbatim to the user. If the tool errors, tell the user to run /connect (Command Code) and retry. $ARGUMENTS",
			};
		},

		// opencode >=1.14.49 resolves the model list here, with auth injected.
		provider: {
			id: "command-code",
			models: async (_provider, ctx) => {
				const key = await resolveKey(async () => {
					const a = ctx.auth;
					return a?.type === "api" && a.key ? { key: a.key } : undefined;
				});
				const split = await loadModels(key);
				return toModelDefs(split.claude, "command-code", "@ai-sdk/anthropic");
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
