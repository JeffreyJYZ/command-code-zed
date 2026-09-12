import { describe, expect, test } from "bun:test";
import conformance from "../../core/conformance.json";
import { canonicalizeModelId, evaluateModelAccess } from "../src/access";
import { bareModel } from "../src/gating";
import {
	compact,
	duration,
	elapsedPct,
	monthlyWindow,
	money,
	paceEta,
	parseIsoUtc,
	pctStr,
	planMonthlyCap,
	planName,
	planRuleMatched,
	relTime,
} from "../src/usage";

// Vectors shared verbatim with cmduse-core (conformance.json). Both language
// ports must satisfy the same cases so the implementations cannot drift.

type MoneyCase = { in: number; out: string };
type CompactCase = { in: number; out: string };
type PctCase = { used: number; cap: number; out: string };
type BareCase = { in: string; out: string };
type CanonCase = { in: string; out: string };
type ElapsedCase = { resetAtMs: number | null; durSecs: number; now: number; out: number | null };
type PaceCase = {
	resetAtMs: number | null;
	durSecs: number;
	used: number;
	cap: number;
	now: number;
	outSecs: number | null;
};
type RelCase = { resetAtMs: number | null; now: number; out: string };
type IsoCase = { in: string; outMs: number | null };
type PlanCase = { id: string; name: string; cap: number | null; matched: boolean };

describe("conformance vectors (shared with cmduse-core)", () => {
	test("money", () => {
		for (const c of conformance.money as MoneyCase[]) {
			expect([c.in, money(c.in)]).toEqual([c.in, c.out]);
		}
	});
	test("compact", () => {
		for (const c of conformance.compact as CompactCase[]) {
			expect([c.in, compact(c.in)]).toEqual([c.in, c.out]);
		}
	});
	test("pct", () => {
		for (const c of conformance.pct as PctCase[]) {
			expect([c.used, c.cap, pctStr(c.used, c.cap)]).toEqual([c.used, c.cap, c.out]);
		}
	});
	test("bareModel", () => {
		for (const c of conformance.bareModel as BareCase[]) {
			expect([c.in, bareModel(c.in)]).toEqual([c.in, c.out]);
		}
	});
	test("canonicalize", () => {
		for (const c of conformance.canonicalize as CanonCase[]) {
			expect([c.in, canonicalizeModelId(c.in)]).toEqual([c.in, c.out]);
		}
	});
	test("elapsedPct", () => {
		for (const c of conformance.elapsedPct as ElapsedCase[]) {
			const got = elapsedPct(c.resetAtMs ?? undefined, c.durSecs, c.now) ?? null;
			expect([c.durSecs, c.now, got]).toEqual([c.durSecs, c.now, c.out]);
		}
	});
	test("paceEta", () => {
		for (const c of conformance.paceEta as PaceCase[]) {
			const got =
				paceEta(c.resetAtMs ?? undefined, c.durSecs, c.used, c.cap, c.now) ?? null;
			expect([c.used, c.cap, got]).toEqual([c.used, c.cap, c.outSecs]);
		}
	});
	test("relTime", () => {
		for (const c of conformance.relTime as RelCase[]) {
			expect([c.resetAtMs, relTime(c.resetAtMs ?? undefined, c.now)]).toEqual([
				c.resetAtMs,
				c.out,
			]);
		}
	});
	test("duration", () => {
		for (const c of conformance.duration as CompactCase[]) {
			expect([c.in, duration(c.in)]).toEqual([c.in, c.out]);
		}
	});
	test("parseIso", () => {
		for (const c of conformance.parseIso as IsoCase[]) {
			const got = parseIsoUtc(c.in);
			expect([c.in, got ?? null]).toEqual([c.in, c.outMs]);
		}
	});
	test("monthlyWindow", () => {
		type MonthlyCase = {
			cap: number;
			remaining: number;
			periodStart: string | null;
			periodEnd: string | null;
			used: number;
			resetAtMs: number | null;
			durSecs: number | null;
		};
		for (const c of conformance.monthlyWindow as MonthlyCase[]) {
			const { window, durSecs } = monthlyWindow(
				c.cap,
				c.remaining,
				c.periodStart ?? undefined,
				c.periodEnd ?? undefined,
			);
			expect([c.cap, window.used]).toEqual([c.cap, c.used]);
			expect([c.periodEnd, window.resetAt ?? null]).toEqual([c.periodEnd, c.resetAtMs]);
			expect([c.periodStart, durSecs ?? null]).toEqual([c.periodStart, c.durSecs]);
		}
	});
	test("plan name + cap", () => {
		for (const c of conformance.plan as PlanCase[]) {
			expect([c.id, planName(c.id)]).toEqual([c.id, c.name]);
			expect([c.id, planMonthlyCap(c.id) ?? null]).toEqual([c.id, c.cap]);
			expect([c.id, planRuleMatched(c.id)]).toEqual([c.id, c.matched]);
		}
	});
	test("gating", () => {
		type GateCase = { model: string; plan: string; unlocked: boolean; allowed: boolean };
		for (const c of conformance.gating as GateCase[]) {
			const got = evaluateModelAccess(c.model, {
				planId: c.plan,
				purchasedCredits: c.unlocked ? 1 : 0,
				freeCredits: 0,
			}).allowed;
			expect([c.model, c.plan, c.unlocked, got]).toEqual([
				c.model,
				c.plan,
				c.unlocked,
				c.allowed,
			]);
		}
	});
});
