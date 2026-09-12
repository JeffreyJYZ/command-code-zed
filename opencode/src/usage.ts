import { credits, subscriptions, usageSummary, type Window } from "./api";
import plansData from "../../core/plans.json";

// Plan table / name rules / monthly caps come from the canonical
// core/plans.json (also loaded by cmduse-core's build.rs) so the two
// languages cannot drift.

// Port of the Zed extension's render logic (../src/lib.rs).

const BAR_WIDTH = 12;

// Rolling-window lengths (seconds); mirrors cmduse_core::FIVE_HOUR_SECS/WEEKLY_SECS.
export const FIVE_HOUR_SECS = 5 * 3600;
export const WEEKLY_SECS = 7 * 86400;

export function money(v: number): string {
	return `$${v.toFixed(2)}`;
}

export function compact(n: number): string {
	if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
	if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
	return `${n}`;
}

export function bar(used: number, cap: number): string {
	const pct = cap > 0 ? Math.min(Math.max(used / cap, 0), 1) : 0;
	const filled = Math.round(pct * BAR_WIDTH);
	return "█".repeat(filled) + "░".repeat(BAR_WIDTH - filled);
}

export function pctStr(used: number, cap: number): string {
	return cap > 0 ? `${Math.round((used / cap) * 100)}%` : "—";
}

export function relTime(resetAtMs: number | undefined, nowSecs: number): string {
	if (resetAtMs === undefined) return "unknown";
	const resetSecs = Math.floor(resetAtMs / 1000);
	if (resetSecs <= nowSecs) return "resetting…";
	const diff = resetSecs - nowSecs;
	const d = Math.floor(diff / 86400);
	const h = Math.floor((diff % 86400) / 3600);
	const m = Math.floor((diff % 3600) / 60);
	if (d > 0) return `${d}d ${h}h`;
	if (h > 0) return `${h}h ${m}m`;
	if (m > 0) return `${m}m`;
	return "<1m";
}

/** Compact "Xh Ym" / "Xd Yh" for a span of seconds. Pure duration — use this
 * (not relTime) for ETAs/countdowns. Mirrors cmduse_core::duration. */
export function duration(secs: number): string {
	const d = Math.floor(secs / 86400);
	const h = Math.floor((secs % 86400) / 3600);
	const m = Math.floor((secs % 3600) / 60);
	if (d > 0) return `${d}d ${h}h`;
	if (h > 0) return `${h}h ${m}m`;
	if (m > 0) return `${m}m`;
	return "<1m";
}

/** % of a rolling window elapsed; window length = durSecs, ends at resetAtMs.
 * undefined when the window hasn't started or the reset is unknown. */
export function elapsedPct(
	resetAtMs: number | undefined,
	durSecs: number,
	nowSecs: number,
): number | undefined {
	if (resetAtMs === undefined) return undefined;
	const resetSecs = Math.floor(resetAtMs / 1000);
	if (resetSecs < durSecs) return undefined; // window start would precede epoch
	const start = resetSecs - durSecs;
	if (nowSecs < start) return undefined;
	const pct = Math.min(Math.max(((nowSecs - start) / durSecs) * 100, 0), 100);
	return Math.round(pct);
}

/** Seconds until spend hits cap at the current rate, if before the reset.
 * Suppressed before 10% of the window has elapsed (flat-rate projection is
 * unreliable that early). undefined = no warning. */
export function paceEta(
	resetAtMs: number | undefined,
	durSecs: number,
	used: number,
	cap: number,
	nowSecs: number,
): number | undefined {
	if (resetAtMs === undefined) return undefined;
	const resetSecs = Math.floor(resetAtMs / 1000);
	if (resetSecs < durSecs) return undefined; // window start would precede epoch
	const start = resetSecs - durSecs;
	if (nowSecs <= start || nowSecs >= resetSecs) return undefined;
	const elapsed = nowSecs - start;
	if (elapsed / durSecs < 0.1) return undefined;
	const rate = used / elapsed;
	if (rate <= 0 || used >= cap) return undefined;
	const secsToCap = (cap - used) / rate;
	if (secsToCap >= resetSecs - nowSecs) return undefined;
	return secsToCap;
}

/** ISO 8601 → epoch ms. Handles trailing Z or a ±HH:MM / ±HHMM offset. */
export function parseIsoUtc(s: string): number | undefined {
	const m = s.match(
		/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2}(?:\.\d+)?)(Z|[+-]\d{2}:?\d{2})?$/,
	);
	if (!m) return undefined;
	const [, y, mo, d, h, mi, sec, off] = m as unknown as [
		string,
		string,
		string,
		string,
		string,
		string,
		string,
		string | undefined,
	];
	let offsetMs = 0;
	if (off && off !== "Z") {
		const sign = off.startsWith("-") ? -1 : 1;
		const digits = off.slice(1).replace(":", "");
		const hh = Number(digits.slice(0, 2));
		const mm = Number(digits.slice(2, 4) || "0");
		offsetMs = sign * (hh * 60 + mm) * 60_000;
	}
	return Date.UTC(+y, +mo - 1, +d, +h, +mi, Math.floor(+sec)) - offsetMs;
}

/** Assemble the monthly window from the plan cap + subscription period:
 * used = cap − remaining (clamped), reset = period end, durSecs = period
 * length. Mirrors cmduse_core::monthly_window (conformance-pinned). */
export function monthlyWindow(
	cap: number,
	remaining: number,
	periodStart?: string,
	periodEnd?: string,
): { window: { used: number; cap: number; resetAt?: number }; durSecs?: number } {
	const used = Math.min(Math.max(cap - remaining, 0), cap);
	const resetAt = periodEnd ? parseIsoUtc(periodEnd) : undefined;
	const start = periodStart ? parseIsoUtc(periodStart) : undefined;
	const durSecs =
		start !== undefined && resetAt !== undefined
			? Math.max(Math.floor((resetAt - start) / 1000), 1)
			: undefined;
	return { window: { used, cap, resetAt }, durSecs };
}

const NAME_RULES = plansData.nameRules as Array<{ needles: string[]; name: string }>;
const DEFAULT_NAME = plansData.defaultName as string;
const CAPS = plansData.caps as Record<string, number | null>;
const PLANS = plansData.plans as Array<{
	name: string;
	price: string;
	monthly: string;
	fiveHour: string;
	weekly: string;
}>;

export function planMonthlyCap(planId: string): number | undefined {
	return CAPS[planName(planId)] ?? undefined;
}

export function planName(planId: string): string {
	const id = planId.toLowerCase();
	for (const rule of NAME_RULES) {
		if (rule.needles.every((n) => id.includes(n))) return rule.name;
	}
	return DEFAULT_NAME;
}

export function windowLine(
	label: string,
	w: Window,
	nowSecs: number,
	durSecs?: number,
): string {
	const status = w.exceeded ? " · **LIMIT EXCEEDED**" : "";
	const elapsed = durSecs !== undefined ? elapsedPct(w.resetAt, durSecs, nowSecs) : undefined;
	const thru = elapsed !== undefined ? ` · window ${elapsed}% elapsed` : "";
	const eta = durSecs !== undefined ? paceEta(w.resetAt, durSecs, w.used, w.cap, nowSecs) : undefined;
	const pace = eta !== undefined ? ` · **on pace to hit cap in ${duration(eta)}**` : "";
	return `**${label}** \`${bar(w.used, w.cap)}\` ${money(w.used)} of ${money(w.cap)} (${pctStr(w.used, w.cap)}) · resets in ${relTime(w.resetAt, nowSecs)}${thru}${pace}${status}\n`;
}

export function plansTable(current: string): string {
	// Mark by exact plan_name match (not substring): "individual-goat"
	// contains "go", so substring matching double-marks the Go row.
	const mine = planName(current);
	let out = "| Plan | Price | Credits/mo | 5-hour | Weekly |\n|---|---|---|---|---|\n";
	for (const { name, price, monthly, fiveHour, weekly } of PLANS) {
		const mark = name === mine ? "**" : "";
		out += `| ${mark}${name}${mark} | ${price}/mo | ${monthly} | ${fiveHour} | ${weekly} |\n`;
	}
	out +=
		"\nWindows throttle only included monthly credits; on-demand (`/extra`) credits are never throttled.\n";
	return out;
}

/** Fetch billing data in parallel and render the markdown dashboard. */
export async function renderUsage(key: string): Promise<string> {
	const nowSecs = Math.floor(Date.now() / 1000);
	const [sub, cr, summary] = await Promise.all([
		subscriptions(key),
		credits(key),
		usageSummary(key).catch(() => undefined),
	]);

	const lines: string[] = [];
	lines.push(`## Command Code — ${planName(sub.planId)} (${sub.status})\n`);
	if (sub.currentPeriodEnd)
		lines.push(`Billing period ends \`${sub.currentPeriodEnd.slice(0, 10)}\`\n`);

	const cap = planMonthlyCap(sub.planId);
	if (cap) {
		lines.push(
			`**Credits:** ${money(cr.credits.monthlyCredits)} / ${money(cap)} monthly · ${money(cr.credits.purchasedCredits ?? 0)} purchased · ${money(cr.credits.freeCredits ?? 0)} free\n`,
		);
	} else {
		lines.push(
			`**Credits remaining:** ${money(cr.credits.monthlyCredits)} monthly · ${money(cr.credits.purchasedCredits ?? 0)} purchased · ${money(cr.credits.freeCredits ?? 0)} free\n`,
		);
	}

	lines.push("### Usage windows");
	if (cap) {
		const { window, durSecs } = monthlyWindow(
			cap,
			cr.credits.monthlyCredits,
			sub.currentPeriodStart,
			sub.currentPeriodEnd,
		);
		lines.push(windowLine("Monthly", { ...window, exceeded: false }, nowSecs, durSecs));
	}
	const five = cr.windowLimits?.fiveHour;
	const weekly = cr.windowLimits?.weekly;
	if (five) lines.push(windowLine("5-hour", five, nowSecs, FIVE_HOUR_SECS));
	if (weekly) lines.push(windowLine("Weekly", weekly, nowSecs, WEEKLY_SECS));
	if (!five && !weekly)
		lines.push("No rolling windows on this plan (pay-as-you-go credits only).\n");

	if (summary) {
		lines.push("### This billing period");
		lines.push("| Metric | Value |", "|---|---|");
		lines.push(`| Requests | ${summary.totalCount ?? "—"} |`);
		lines.push(`| Cost | ${summary.totalCost !== undefined ? money(summary.totalCost) : "—"} |`);
		lines.push(
			`| Tokens in / out | ${summary.totalTokensIn !== undefined ? compact(summary.totalTokensIn) : "—"} / ${summary.totalTokensOut !== undefined ? compact(summary.totalTokensOut) : "—"} |`,
		);
		lines.push(
			`| Success rate | ${summary.successRate !== undefined ? `${Math.round(summary.successRate)}%` : "—"} |`,
		);
		lines.push("");
	}

	lines.push("### All plans");
	lines.push(plansTable(sub.planId));
	return lines.join("\n");
}
