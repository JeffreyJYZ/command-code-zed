import { type Category, canonicalizeModelId, MODEL_CATEGORIES, PLAN_RULES } from "./gating";

export { canonicalizeModelId, PLAN_RULES };

// Port of Command Code CLI's evaluateModelAccess (see scripts/extract-gating.ts
// for provenance). Any plan with purchased/free credits gets everything; unknown
// plan ids get everything (API enforces the real limit anyway).

export type PlanLike = {
	planId: string;
	purchasedCredits: number;
	freeCredits: number;
};

/** Models the API hard-blocks (MODEL_NOT_IN_PLAN 403) per planId, beyond the
 * category rules. Empirically probed — CLI's bundle misses these because it
 * tracks "category" via serving lane, not per-model plan entitlements. */
const HARD_BLOCKED: Record<string, string[]> = {
	"individual-goat": [
		"meta/muse-spark-1.1",
		"google/gemini-3.5-flash",
		"google/gemini-3.6-flash",
		"google/gemini-3.5-flash-lite",
		"google/gemini-3.1-flash-lite",
	],
	"individual-go": [
		"meta/muse-spark-1.1",
		"google/gemini-3.5-flash",
		"google/gemini-3.6-flash",
		"google/gemini-3.5-flash-lite",
		"google/gemini-3.1-flash-lite",
		"meta/muse-spark-1.2",
		"meta/muse-spark-1.2-contributor",
		"meta/muse-spark-1.3",
		"meta/muse-spark-1.3-contributor",
	],
};

export function evaluateModelAccess(model: string, plan: PlanLike): { allowed: boolean } {
	if (plan.purchasedCredits > 0 || plan.freeCredits > 0) return { allowed: true };
	if (!plan.planId) return { allowed: true };
	const canonical = canonicalizeModelId(model);
	if ((HARD_BLOCKED[plan.planId] ?? []).some((m) => m.toLowerCase() === canonical.toLowerCase())) {
		return { allowed: false };
	}
	const rules = PLAN_RULES[plan.planId];
	if (!rules) return { allowed: true };
	// CLI canonicalizes through deprecated aliases only; models newer than the
	// installed CLI share the prefix-category of the closest known sibling
	// (claude-fable-5-1 → claude-fable-5 → premium). Unknown non-claude ids with
	// no sibling default to opensource (matches how Command Code adds open models).
	const category = MODEL_CATEGORIES[canonical] ?? siblingCategory(canonical);
	if (!category) return { allowed: true };
	// blockedModels are provider-qualified ("anthropic:claude-opus-5"); we only serve
	// via command-code lanes, so match on the bare model id portion.
	const blocked = rules.blockedModels.some((b) => {
		const bare = b.includes(":") ? b.slice(b.indexOf(":") + 1) : b;
		return bare.toLowerCase() === canonical.toLowerCase();
	});
	if (blocked) return { allowed: false };
	if (!rules.allowedCategories.includes(category)) return { allowed: false };
	return { allowed: true };
}

/** Filter a live model list down to what the plan allows. */
export function filterByPlan<T extends { id: string }>(models: T[], plan: PlanLike): T[] {
	return models.filter((m) => evaluateModelAccess(m.id, plan).allowed);
}

const KNOWN_KEYS = Object.keys(MODEL_CATEGORIES);

/** Category of the longest known model id that is a prefix of this one
 * ("claude-fable-5-1" → "claude-fable-5" → premium). For version-bumped ids
 * where no known id is a literal prefix (e.g. "claude-sonnet-6" with no
 * "claude-sonnet-" entry), return undefined so the API decides.
 * ponytail: heuristic, not CLI parity — CLI hard-rejects unknown ids, we
 * default-allow; the API enforces the real gate. Wrong guess only shows/hides
 * one model. Stem-match fallback was tried and removed: it picked the wrong
 * sibling deterministically (e.g. muse-spark-1.3 → muse-spark-1.1 premium). */
function siblingCategory(model: string): Category | undefined {
	const lower = model.toLowerCase();
	let best: string | undefined;
	let bestLen = 0;
	for (const k of KNOWN_KEYS) {
		if (lower.startsWith(k.toLowerCase()) && k.length > bestLen) {
			best = k;
			bestLen = k.length;
		}
	}
	return best ? MODEL_CATEGORIES[best] : undefined;
}
