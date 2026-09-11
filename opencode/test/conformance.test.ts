import { describe, expect, test } from "bun:test";
import conformance from "../../core/conformance.json";
import { evaluateModelAccess } from "../src/access";
import {
	compact,
	money,
	parseIsoUtc,
	planMonthlyCap,
	planName,
	relTime,
} from "../src/usage";

// Vectors shared verbatim with cmduse-core (conformance.json). Both language
// ports must satisfy the same cases so the implementations cannot drift.

type MoneyCase = { in: number; out: string };
type CompactCase = { in: number; out: string };
type RelCase = { resetAtMs: number | null; now: number; out: string };
type IsoCase = { in: string; outMs: number | null };
type PlanCase = { id: string; name: string; cap: number | null };

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
	test("relTime", () => {
		for (const c of conformance.relTime as RelCase[]) {
			expect([c.resetAtMs, relTime(c.resetAtMs ?? undefined, c.now)]).toEqual([
				c.resetAtMs,
				c.out,
			]);
		}
	});
	test("parseIso", () => {
		for (const c of conformance.parseIso as IsoCase[]) {
			const got = parseIsoUtc(c.in);
			expect([c.in, got ?? null]).toEqual([c.in, c.outMs]);
		}
	});
	test("plan name + cap", () => {
		for (const c of conformance.plan as PlanCase[]) {
			expect([c.id, planName(c.id)]).toEqual([c.id, c.name]);
			expect([c.id, planMonthlyCap(c.id) ?? null]).toEqual([c.id, c.cap]);
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
