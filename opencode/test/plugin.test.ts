import { describe, expect, test } from "bun:test";
import { canonicalizeModelId, evaluateModelAccess } from "../src/access";
import { MODEL_CATEGORIES, PLAN_RULES } from "../src/gating";
import { isClaude, splitModels } from "../src/models";
import { bar, FIVE_HOUR_SECS, pctStr, plansTable, windowLine } from "../src/usage";

// money/compact/relTime/parseIsoUtc/planName/planMonthlyCap and the core gating
// behavior are pinned by conformance.test.ts (shared vectors). This file covers
// only TS-specific logic: canonicalization, wire split, table/status rendering,
// and gating edges not represented as vectors.

const goat = { planId: "individual-goat", purchasedCredits: 0, freeCredits: 0 };

describe("canonicalizeModelId", () => {
	test("exact", () => {
		expect(canonicalizeModelId("claude-sonnet-5")).toBe("claude-sonnet-5");
	});
	test("case-insensitive", () => {
		expect(canonicalizeModelId("minimaxai/minimax-m2.5")).toBe("MiniMaxAI/MiniMax-M2.5");
	});
	test("deprecated alias", () => {
		expect(canonicalizeModelId("claude-opus-4-6")).toBe("claude-opus-4-7");
		expect(canonicalizeModelId("claude-haiku-4-5")).toBe("claude-haiku-4-5-20251001");
	});
	test("date suffix stripped", () => {
		expect(canonicalizeModelId("claude-opus-4-7-20251101")).toBe("claude-opus-4-7");
	});
	test("unknown passes through", () => {
		expect(canonicalizeModelId("totally-new-model")).toBe("totally-new-model");
	});
});

describe("gating tables match CLI", () => {
	test("go plan blocks muse-spark-1.2 + grok-4.6", () => {
		const rules = PLAN_RULES["individual-go"];
		expect(rules?.allowedCategories).toEqual(["opensource"]);
		expect(rules?.blockedModels).toContain("vercel-ai-gateway:meta/muse-spark-1.2");
		expect(rules?.blockedModels).toContain("vercel-ai-gateway:xai/grok-4.6");
	});
	test("pro blocks opus + fable, allows sonnet", () => {
		const rules = PLAN_RULES["individual-pro"];
		expect(rules?.blockedModels).toContain("anthropic:claude-opus-5");
		expect(rules?.blockedModels).toContain("anthropic:claude-fable-5");
	});
	test("categories", () => {
		expect(MODEL_CATEGORIES["claude-sonnet-5"]).toBe("premium");
		expect(MODEL_CATEGORIES["deepseek/deepseek-v4-flash"]).toBe("opensource");
	});
});

describe("evaluateModelAccess (edges beyond conformance vectors)", () => {
	test("newer sibling of premium model inherits premium (claude-fable-5-1)", () => {
		expect(evaluateModelAccess("claude-fable-5-1", goat).allowed).toBe(false);
	});
	test("version-bumped id with no prefix sibling defaults to allow (API enforces)", () => {
		expect(evaluateModelAccess("claude-sonnet-6", goat).allowed).toBe(true);
	});
	test("unprefixed unknown model on gated plan defaults opensource", () => {
		expect(evaluateModelAccess("newvendor/new-open-model", goat).allowed).toBe(true);
	});
	test("empirically hard-blocked model on GOAT (API 403 MODEL_NOT_IN_PLAN)", () => {
		expect(evaluateModelAccess("meta/muse-spark-1.1", goat).allowed).toBe(false);
	});
	test("purchased credits override hard block", () => {
		expect(
			evaluateModelAccess("meta/muse-spark-1.1", { ...goat, purchasedCredits: 5 }).allowed,
		).toBe(true);
	});
});

describe("wire split", () => {
	test("claude detection", () => {
		expect(isClaude("claude-sonnet-5")).toBe(true);
		expect(isClaude("claude-opus-4-7")).toBe(true);
		expect(isClaude("openai/gpt-5.5")).toBe(false);
		expect(isClaude("deepseek/deepseek-v4-flash")).toBe(false);
	});
	test("splitModels sorts lanes", () => {
		const { claude, open } = splitModels([
			{ id: "claude-sonnet-5", name: "Sonnet", contextLength: 1 },
			{ id: "gpt-5.5", name: "GPT", contextLength: 1 },
		]);
		expect(claude.map((m) => m.id)).toEqual(["claude-sonnet-5"]);
		expect(open.map((m) => m.id)).toEqual(["gpt-5.5"]);
	});
});

describe("usage render", () => {
	test("bar", () => {
		expect(bar(0, 10)).toBe("░".repeat(12));
		expect(bar(10, 10)).toBe("█".repeat(12));
		expect(bar(6, 10).split("█").length - 1).toBe(7);
	});
	test("pctStr", () => {
		expect(pctStr(5, 10)).toBe("50%");
		expect(pctStr(5, 0)).toBe("—");
	});
	test("windowLine", () => {
		const line = windowLine("5-hour", { used: 7, cap: 14, resetAt: 120_000 }, 60);
		expect(line).toContain("**5-hour**");
		expect(line).not.toContain("LIMIT EXCEEDED");
		expect(line).toContain("$7.00 of $14.00");
		const exceeded = windowLine(
			"5-hour",
			{ used: 15, cap: 14, exceeded: true, resetAt: 120_000 },
			60,
		);
		expect(exceeded).toContain("LIMIT EXCEEDED");
	});
	test("windowLine adds elapsed% and pace when duration is known", () => {
		const now = 1_000_000;
		// 10% elapsed, spend rate on track to hit cap before reset
		const pace = windowLine(
			"5-hour",
			{ used: 5, cap: 10, resetAt: 1_016_200_000 },
			now,
			FIVE_HOUR_SECS,
		);
		expect(pace).toContain("window 10% elapsed");
		expect(pace).toContain("on pace to hit cap in 30m");
	});
	test("plansTable marks current", () => {
		const t = plansTable("individual-goat");
		expect(t).toContain("**GOAT**");
		expect(t).toContain("| Plan | Price |");
	});
});
