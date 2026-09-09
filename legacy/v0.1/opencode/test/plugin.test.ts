import { describe, expect, test } from "bun:test";
import { canonicalizeModelId, evaluateModelAccess } from "../src/access";
import { MODEL_CATEGORIES, PLAN_RULES } from "../src/gating";
import { isClaude, splitModels } from "../src/models";
import {
	bar,
	compact,
	money,
	parseIsoUtc,
	pctStr,
	planMonthlyCap,
	planName,
	plansTable,
	relTime,
	windowLine,
} from "../src/usage";

const credits = (n: number) => ({
	planId: "individual-goat",
	purchasedCredits: n,
	freeCredits: 0,
});

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

describe("evaluateModelAccess", () => {
	test("purchased credits unlock everything", () => {
		expect(evaluateModelAccess("claude-opus-5", credits(5)).allowed).toBe(true);
	});
	test("go plan: opensource ok, premium blocked", () => {
		const goat = {
			planId: "individual-go",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("deepseek/deepseek-v4-flash", goat).allowed).toBe(true);
		expect(evaluateModelAccess("claude-sonnet-5", goat).allowed).toBe(false);
		expect(evaluateModelAccess("meta/muse-spark-1.2", goat).allowed).toBe(false);
	});
	test("goat plan: opensource ok, premium blocked", () => {
		const goat = {
			planId: "individual-goat",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("z-ai/glm-5.3-flash", goat).allowed).toBe(true);
		expect(evaluateModelAccess("claude-sonnet-5", goat).allowed).toBe(false);
	});
	test("pro plan: sonnet ok, opus blocked", () => {
		const pro = {
			planId: "individual-pro",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("claude-sonnet-5", pro).allowed).toBe(true);
		expect(evaluateModelAccess("claude-opus-5", pro).allowed).toBe(false);
	});
	test("max allows everything", () => {
		const max = {
			planId: "individual-max",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("claude-opus-5", max).allowed).toBe(true);
	});
	test("unknown model/plan defaults allow", () => {
		expect(evaluateModelAccess("brand-new-model", credits(0)).allowed).toBe(true);
		expect(
			evaluateModelAccess("claude-opus-5", {
				planId: "individual-free-tier",
				purchasedCredits: 0,
				freeCredits: 0,
			}).allowed,
		).toBe(true);
	});
	test("newer sibling of premium model inherits premium (claude-fable-5-1)", () => {
		const goat = {
			planId: "individual-goat",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("claude-fable-5-1", goat).allowed).toBe(false);
	});
	test("version-bumped id with no prefix sibling defaults to allow (API enforces)", () => {
		const goat = {
			planId: "individual-goat",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("claude-sonnet-6", goat).allowed).toBe(true);
		expect(evaluateModelAccess("meta/muse-spark-1.3", goat).allowed).toBe(true);
	});
	test("unprefixed unknown model on gated plan defaults opensource", () => {
		const goat = {
			planId: "individual-goat",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("newvendor/new-open-model", goat).allowed).toBe(true);
	});
	test("empirically hard-blocked models on GOAT (API 403 MODEL_NOT_IN_PLAN)", () => {
		const goat = {
			planId: "individual-goat",
			purchasedCredits: 0,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("meta/muse-spark-1.1", goat).allowed).toBe(false);
		expect(evaluateModelAccess("google/gemini-3.5-flash", goat).allowed).toBe(false);
		expect(evaluateModelAccess("google/gemini-3.5-flash-lite", goat).allowed).toBe(false);
	});
	test("purchased credits override hard block", () => {
		const goat = {
			planId: "individual-goat",
			purchasedCredits: 5,
			freeCredits: 0,
		};
		expect(evaluateModelAccess("meta/muse-spark-1.1", goat).allowed).toBe(true);
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
	test("money/compact", () => {
		expect(money(1.5)).toBe("$1.50");
		expect(compact(28_129_791)).toBe("28.1M");
		expect(compact(12_345)).toBe("12.3K");
		expect(compact(42)).toBe("42");
	});
	test("bar", () => {
		expect(bar(0, 10)).toBe("░".repeat(12));
		expect(bar(10, 10)).toBe("█".repeat(12));
		expect(bar(6, 10).split("█").length - 1).toBe(7);
	});
	test("pctStr", () => {
		expect(pctStr(5, 10)).toBe("50%");
		expect(pctStr(5, 0)).toBe("—");
	});
	test("relTime", () => {
		const now = 1_000_000;
		expect(relTime((now + 3600) * 1000, now)).toBe("1h 0m");
		expect(relTime((now + 120) * 1000, now)).toBe("2m");
		expect(relTime((now - 5) * 1000, now)).toBe("resetting…");
		expect(relTime(undefined, now)).toBe("unknown");
	});
	test("parseIsoUtc", () => {
		expect(parseIsoUtc("2026-09-27T12:23:00.000Z")).toBe(1_790_511_780_000);
		expect(parseIsoUtc("garbage")).toBeUndefined();
	});
	test("plan caps + names", () => {
		expect(planMonthlyCap("individual-goat")).toBe(70);
		expect(planMonthlyCap("individual-provider")).toBeUndefined();
		expect(planName("individual-goat")).toBe("GOAT");
		expect(planName("individual-max-20")).toBe("Max 20x");
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
	test("plansTable marks current", () => {
		const t = plansTable("individual-goat");
		expect(t).toContain("**GOAT**");
		expect(t).toContain("| Plan | Price |");
	});
});
