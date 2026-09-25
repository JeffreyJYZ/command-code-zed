import { describe, expect, test } from "bun:test";
import conformance from "../../core/conformance.json";
import { canonicalizeModelId, evaluateModelAccess } from "../src/access";
import { bareModel } from "../src/gating";
import { elapsedPct } from "../src/sidebar/windows";

// Vectors shared verbatim with cmduse-core (conformance.json). Since 0.2.0 the
// plugin delegates usage rendering to the cmduse CLI, so the TS port only
// covers the model-gating access layer — bareModel, canonicalize, and gating.
// The Rust core remains the single implementation for money/compact/pct/
// duration/rel_time/parse_iso_utc/elapsed_pct/pace_eta/monthly_window/
// plan_name/plan_monthly_cap (its own test suite asserts those vectors).

type BareCase = { in: string; out: string };
type CanonCase = { in: string; out: string };

describe("conformance vectors (shared with cmduse-core)", () => {
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
	test("elapsedPct", () => {
		type Case = { resetAtMs: number | null; durSecs: number; now: number; out: number | null };
		for (const c of conformance.elapsedPct as Case[]) {
			const resetAt = c.resetAtMs === null ? undefined : c.resetAtMs;
			const expected = c.out === null ? undefined : c.out;
			expect([c.resetAtMs, c.durSecs, c.now, elapsedPct(resetAt, c.durSecs, c.now)]).toEqual([
				c.resetAtMs,
				c.durSecs,
				c.now,
				expected,
			]);
		}
	});
});
