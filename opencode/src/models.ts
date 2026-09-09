import { filterByPlan, type PlanLike } from "./access";
import { credits, getPlanInfo, providerModels } from "./api";

export type CmdModel = { id: string; name: string; contextLength: number };

export type ModelSplit = {
	claude: CmdModel[];
	open: CmdModel[];
};

const CACHE_TTL_MS = 5 * 60 * 1000;
let cache: { at: number; models: CmdModel[] } | null = null;

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
	let models: CmdModel[];
	if (cache && Date.now() - cache.at < CACHE_TTL_MS) {
		models = cache.models;
	} else {
		try {
			const resp = await providerModels(key);
			models = (resp.data ?? []).map((m) => ({
				id: m.id,
				name: m.name ?? m.id,
				contextLength: m.context_length ?? 0,
			}));
			cache = { at: Date.now(), models };
		} catch {
			if (cache) {
				models = cache.models;
			} else {
				throw new Error(
					"Could not fetch Command Code model list (https://api.commandcode.ai/provider/v1/models). Check network/API key.",
				);
			}
		}
	}

	let plan: PlanLike = { planId: "", purchasedCredits: 0, freeCredits: 0 };
	try {
		const sub = await getPlanInfo(key);
		const cr = await credits(key);
		plan = {
			planId: sub.planId,
			purchasedCredits: cr.credits.purchasedCredits ?? 0,
			freeCredits: cr.credits.freeCredits ?? 0,
		};
	} catch {
		// gating needs billing API; if unreachable, show everything (API enforces real limits)
	}
	const allowed = filterByPlan(models, plan);
	const { claude, open } = splitModels(allowed);
	return { claude, open };
}
