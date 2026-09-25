// Pure row builder for the "Command Code" session-sidebar section.
//
// Rows are `[label, value]` pairs plus standalone banners (`[message, ""]`),
// the same shape cmd-provider's deals panel uses. Keeping this pure makes the
// panel trivial to test and host-agnostic (v1 and v2 pass the same inputs).
import { MODEL_CATEGORIES, canonicalizeModelId, type Category } from "../gating"
import { FIVE_HOUR_SECS, WEEKLY_SECS, elapsedPct } from "./windows"

export type SidebarRow = [label: string, value: string]

export interface ModelMeta {
	key: string
	name: string
	tier?: Category
	allowance?: number
	rates?: { input: number; output: number; cacheRead: number; cacheWrite?: number | null }
	intelligence?: number
	tps?: number
}

export interface WindowUsage {
	cap?: number
	used?: number
	resetAt?: number
}

/** One model's usage for the account's current billing period. */
export interface ModelUsage {
	requests: number
	cost: number
}

export interface Usage {
	plan?: string
	monthlyCap?: number
	monthlyCredits?: number
	fiveHour?: WindowUsage
	weekly?: WindowUsage
	periodEnd?: string
	requests?: number
	cost?: number
}

const TIER_DISPLAY: Readonly<Record<Category, string>> = {
	opensource: "open source",
	premium: "premium",
}

/** Money at credit scale: cents, but "$60" not "$60.00". */
export function money(value: number): string {
	const rounded = Math.round(value * 100) / 100
	return `$${Number.isInteger(rounded) ? rounded : rounded.toFixed(2)}`
}

/** Rate scale needs more precision than credit scale ($0.0036). */
export function rate(value: number): string {
	if (value === 0) return "$0"
	return `$${Number(value.toFixed(4))}`
}

export function count(value: number): string {
	if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
	if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`
	return String(Math.round(value))
}

/** "3h 12m" / "1d 20h" / "<1m" for a reset epoch. */
export function until(epoch: number | undefined, now = Date.now()): string {
	if (!epoch) return ""
	const secs = Math.max(0, Math.round((epoch - now) / 1000))
	if (secs < 60) return "<1m"
	const mins = Math.floor(secs / 60)
	if (mins < 60) return `${mins}m`
	const hours = Math.floor(mins / 60)
	if (hours < 24) return mins % 60 ? `${hours}h ${mins % 60}m` : `${hours}h`
	const days = Math.floor(hours / 24)
	return hours % 24 ? `${days}d ${hours % 24}h` : `${days}d`
}

function windowRow(
	label: string,
	w: WindowUsage | undefined,
	now: number,
	durSecs: number,
): SidebarRow | undefined {
	if (!w || typeof w.cap !== "number" || w.cap <= 0) return undefined
	const used = typeof w.used === "number" ? w.used : 0
	const pct = Math.round((used / w.cap) * 100)
	const elapsed = elapsedPct(w.resetAt, durSecs, Math.floor(now / 1000))
	const reset = until(w.resetAt, now)
	const parts = [
		`${money(used)} / ${money(w.cap)} (${pct}%)`,
		...(elapsed === undefined ? [] : [`${elapsed}% elapsed`]),
		...(reset ? [`resets ${reset}`] : []),
	]
	return [label, parts.join(" · ")]
}

/** Plan + rolling windows + period totals, from a cmduse snapshot. */
export function usageRows(usage: Usage | undefined, now = Date.now()): SidebarRow[] {
	if (!usage) return []
	const rows: SidebarRow[] = []
	if (usage.plan) {
		const credits =
			typeof usage.monthlyCap === "number" ? ` · $${usage.monthlyCap}/mo credits` : ""
		rows.push(["Plan", `${usage.plan}${credits}`])
	}
	if (typeof usage.monthlyCap === "number" && typeof usage.monthlyCredits === "number") {
		const used = usage.monthlyCap - usage.monthlyCredits
		const pct = usage.monthlyCap > 0 ? Math.round((used / usage.monthlyCap) * 100) : 0
		rows.push(["Monthly", `${money(used)} / ${money(usage.monthlyCap)} (${pct}%)`])
	}
	const five = windowRow("5-hour", usage.fiveHour, now, FIVE_HOUR_SECS)
	if (five) rows.push(five)
	const week = windowRow("Weekly", usage.weekly, now, WEEKLY_SECS)
	if (week) rows.push(week)
	if (typeof usage.requests === "number" || typeof usage.cost === "number") {
		const parts: string[] = []
		if (typeof usage.requests === "number") parts.push(`${count(usage.requests)} requests`)
		if (typeof usage.cost === "number") parts.push(money(usage.cost))
		rows.push(["Period", parts.join(" · ")])
	}
	return rows
}

/** Active model's allowance, rates and benchmarks, from mpc's catalog; its
 * period usage (when the store has it) rides directly under the model name. */
export function modelRows(meta: ModelMeta | undefined, usage?: ModelUsage): SidebarRow[] {
	if (!meta) return []
	const rows: SidebarRow[] = [["Model", meta.name]]
	if (usage) {
		// CommandCode is subscription-billed, so opencode records cost 0 for
		// its models: show spend only when the harness actually priced it.
		const spent = usage.cost > 0 ? ` · ${money(usage.cost)}` : ""
		rows.push(["Usage (this model)", `${count(usage.requests)} req${spent}`])
	}
	if (meta.tier) rows.push(["Tier", TIER_DISPLAY[meta.tier]])
	if (typeof meta.allowance === "number") rows.push(["Allowance", `${money(meta.allowance)}/mo`])
	if (meta.rates) {
		rows.push(["Rates", `${rate(meta.rates.input)}/${rate(meta.rates.output)} in/out`])
		rows.push(["Cache read", rate(meta.rates.cacheRead)])
	}
	if (typeof meta.intelligence === "number") rows.push(["Intelligence", String(meta.intelligence)])
	if (typeof meta.tps === "number") rows.push(["Tok/s", String(meta.tps)])
	return rows
}

/**
 * Canonical key for a model id, mirroring mpc's normalizeKey: lowercase, drop
 * the vendor prefix and any parenthetical, strip punctuation. Used to match the
 * session's active model against the mpc catalog.
 */
export function modelKey(modelId: string): string {
	const bare = modelId.toLowerCase().replace(/\([^)]*\)/g, " ")
	const tail = bare.split("/").pop() ?? bare
	return tail.replace(/[^a-z0-9]+/g, "")
}

/** Tier lookup for a model id, from the canonical gating snapshot. */
export function tierFor(modelId: string): Category | undefined {
	return MODEL_CATEGORIES[canonicalizeModelId(modelId)]
}
