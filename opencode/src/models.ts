import { evaluateModelAccess, type PlanLike } from "./access";
import { credits, providerModels, subscriptions } from "./api";
import { warnIfGatingStale } from "./gating";

export type CmdModel = { id: string; name: string; contextLength: number };

export type ModelSplit = {
	claude: CmdModel[];
	open: CmdModel[];
};

const CACHE_TTL_MS = 5 * 60 * 1000;
let cache: { key: string; at: number; models: CmdModel[] } | null = null;

// ponytail: process-lifetime cache only; restart refetches. Add disk cache if
// startup fetch latency ever annoys offline users.
export function isClaude(id: string): boolean {
	const bare = id.includes("/") ? id.slice(id.lastIndexOf("/") + 1) : id;
	return bare.toLowerCase().startsWith("claude");
}

export function splitModels(models: CmdModel[]): {
	claude: CmdModel[];
	open: CmdModel[];
} {
	const claude: CmdModel[] = [];
	const open: CmdModel[] = [];
	for (const m of models) (isClaude(m.id) ? claude : open).push(m);
	return { claude, open };
}

/** Fetch live model list, apply plan gating, split by wire protocol. */
export async function loadModels(key: string): Promise<ModelSplit> {
	warnIfGatingStale();
	// Cache is keyed by API key: CMD_API_KEY lets one process serve multiple
	// accounts, and account A's model list must not leak into account B.
	let models: CmdModel[];
	const fresh = cache && cache.key === key && Date.now() - cache.at < CACHE_TTL_MS;
	if (fresh) {
		models = cache!.models;
	} else {
		try {
			const resp = await providerModels(key);
			models = (resp.data ?? []).map((m) => ({
				id: m.id,
				name: m.name ?? m.id,
				contextLength: m.context_length ?? 0,
			}));
			cache = { key, at: Date.now(), models };
		} catch (e) {
			if (cache && cache.key === key) {
				models = cache.models;
			} else {
				throw new Error(
					"Could not fetch Command Code model list (https://api.commandcode.ai/provider/v1/models). Check network/API key.",
					{ cause: e },
				);
			}
		}
	}

	let plan: PlanLike = { planId: "", purchasedCredits: 0, freeCredits: 0 };
	try {
		const [sub, cr] = await Promise.all([subscriptions(key), credits(key)]);
		plan = {
			planId: sub.planId,
			purchasedCredits: cr.credits.purchasedCredits ?? 0,
			freeCredits: cr.credits.freeCredits ?? 0,
		};
	} catch {
		// gating needs billing API; if unreachable, show everything (API enforces real limits)
	}
	const allowed = models.filter((m) => evaluateModelAccess(m.id, plan).allowed);
	const { claude, open } = splitModels(allowed);
	return { claude, open };
}
