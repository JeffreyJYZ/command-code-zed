import { credits, subscriptions, usageSummary, type Window } from "./api";
import plansData from "../../core/plans.json";

// Plan table / name rules / monthly caps come from the canonical
// core/plans.json (also loaded by cmduse-core's build.rs) so the two
// languages cannot drift.

// Port of the Zed extension's render logic (../src/lib.rs).

const BAR_WIDTH = 12;

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

export function windowLine(label: string, w: Window, nowSecs: number): string {
	const status = w.exceeded ? " · **LIMIT EXCEEDED**" : "";
	return `**${label}** \`${bar(w.used, w.cap)}\` ${money(w.used)} of ${money(w.cap)} (${pctStr(w.used, w.cap)}) · resets in ${relTime(w.resetAt, nowSecs)}${status}\n`;
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
		const used = Math.min(Math.max(cap - cr.credits.monthlyCredits, 0), cap);
		const resetAt = sub.currentPeriodEnd ? parseIsoUtc(sub.currentPeriodEnd) : undefined;
		lines.push(windowLine("Monthly", { used, cap, resetAt }, nowSecs));
	}
	const five = cr.windowLimits?.fiveHour;
	const weekly = cr.windowLimits?.weekly;
	if (five) lines.push(windowLine("5-hour", five, nowSecs));
	if (weekly) lines.push(windowLine("Weekly", weekly, nowSecs));
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
