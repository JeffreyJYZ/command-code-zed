// Gating DATA lives in ../../core/gating.json — single source shared with
// cmduse-core's build.rs (Rust CLI uses the same tables). This file only adds
// the TS types + canonicalization, and re-exports the data. Regen the JSON:
// `bun run extract` (needs the installed Command Code CLI).
import data from "../../core/gating.json";

export type Category = "opensource" | "premium";

/** model id -> category (models absent from this table have unknown category) */
export const MODEL_CATEGORIES = data.categories as Record<string, Category>;

/** planId -> access rules; plan ids absent from this table get everything */
export const PLAN_RULES = data.plans as Record<
	string,
	{ allowedCategories: Category[]; blockedModels: string[] }
>;

/** Models the API hard-blocks (403 MODEL_NOT_IN_PLAN) per plan, beyond the
 * category rules. Empirically probed — the CLI bundle misses these because it
 * tracks category via serving lane, not per-model plan entitlements. */
export const HARD_BLOCKED = data.hardBlocked as Record<string, string[]>;

/** canonical known model ids (lowercase compare) */
const KNOWN_MODELS = data.knownModels as string[];

/** deprecated/aliased model id -> canonical id */
const MODEL_ALIASES = data.aliases as Record<string, string>;

/** strip a trailing date suffix like -20251101 before aliasing (mirrors CLI kr regex) */
function findKnown(s: string): string | undefined {
	const k = s.toLowerCase();
	return KNOWN_MODELS.find((m) => m.toLowerCase() === k);
}

export function canonicalizeModelId(model: string): string {
	const stripDate = (s: string) => s.replace(/[-@]\d{8}$/, "");
	const direct = findKnown(model);
	if (direct) return direct;
	const aliased = MODEL_ALIASES[model.toLowerCase()];
	if (aliased) return findKnown(aliased) ?? model;
	return findKnown(stripDate(model)) ?? model;
}

/** Bare model id with any provider qualifier stripped: everything after the
 * FIRST colon ("anthropic:claude-opus-5" → "claude-opus-5"). Matches
 * cmduse-core's `bare_model`; a model id that itself contains ':' keeps it. */
export function bareModel(blocked: string): string {
	const i = blocked.indexOf(":");
	return i >= 0 ? blocked.slice(i + 1) : blocked;
}
