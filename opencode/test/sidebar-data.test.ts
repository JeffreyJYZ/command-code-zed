import { describe, expect, test } from "bun:test"
import { Database } from "bun:sqlite"
import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { SPAWN_OPTIONS, parseMpcJson, parseUsageJson } from "../src/sidebar/data"
import { loadModelUsage, periodStart } from "../src/sidebar/usageDb"

const USAGE = JSON.stringify({
	error: null,
	plan: "GOAT",
	monthlyCap: 70,
	monthlyCredits: 19.44,
	periodEnd: "2026-09-27",
	fiveHour: { cap: 14, exceeded: false, resetAt: 1790258372737, used: 1.17 },
	weekly: { cap: 35, exceeded: false, resetAt: 1790331314090, used: 12.43 },
	summary: { requests: 6296, cost: 46.31, tokensIn: 1, tokensOut: 2 },
})

describe("parseUsageJson", () => {
	test("lifts the fields the sidebar needs", () => {
		const usage = parseUsageJson(USAGE)
		expect(usage.plan).toBe("GOAT")
		expect(usage.monthlyCap).toBe(70)
		expect(usage.monthlyCredits).toBe(19.44)
		expect(usage.fiveHour?.cap).toBe(14)
		expect(usage.weekly?.used).toBe(12.43)
		expect(usage.requests).toBe(6296)
		expect(usage.cost).toBe(46.31)
	})
	test("tolerates a partial payload", () => {
		expect(parseUsageJson("{}")).toEqual({
			plan: undefined,
			monthlyCap: undefined,
			monthlyCredits: undefined,
			fiveHour: undefined,
			weekly: undefined,
			periodEnd: undefined,
			requests: undefined,
			cost: undefined,
		})
	})
})

const MPC = JSON.stringify({
	plans: { cc: { label: "GOAT" } },
	rows: [
		{
			key: "deepseekv41flash",
			name: "DeepSeek V4.1 Flash",
			cc: {
				allowance: 60,
				pricing: { input: 0.15, output: 0.6, cacheRead: 0.003, cacheWrite: null },
				ability: 39.5,
				tps: 237,
			},
		},
		{ key: "opencodeonly", name: "Some OC Model" },
	],
})

describe("parseMpcJson", () => {
	test("keeps CommandCode rows and maps the fields", () => {
		const meta = parseMpcJson(MPC)
		// indexed under mpc's key and ours; identical here, so one entry
		expect(meta.size).toBe(1)
		const entry = meta.get("deepseekv41flash")
		expect(entry?.allowance).toBe(60)
		expect(entry?.intelligence).toBe(39.5)
		expect(entry?.tps).toBe(237)
		expect(entry?.rates?.cacheRead).toBe(0.003)
		expect(entry?.tier).toBeUndefined() // tier is resolved from the model id at render
	})
	test("drops rows with no CommandCode side", () => {
		const meta = parseMpcJson(MPC)
		expect([...meta.values()].some((m) => m.name === "Some OC Model")).toBe(false)
	})
})

test("parseMpcJson keeps the mpc key and a local key for lookup", () => {
	const meta = parseMpcJson(
		JSON.stringify({
			rows: [
				{
					key: "tencenthy3",
					name: "Tencent Hy3",
					cc: { allowance: 70, pricing: { input: 0.14, output: 0.58, cacheRead: 0.035 } },
				},
			],
		}),
	)
	expect(meta.get("tencenthy3")).toBeDefined()
	expect(meta.get("tencenthy3")).toBe(meta.get("tencenthy3")!)
})

// Regression: cmduse's snapshot() writes a "fetching usage…" spinner directly
// to /dev/tty, so without `detached` (no controlling terminal) it overpaints
// the opencode TUI prompt on every poll.
test("spawns cmduse with no controlling terminal", () => {
	expect(SPAWN_OPTIONS.detached).toBe(true)
})

/** Minimal stand-in for opencode's `message` store. */
function fixtureDb(): string {
	const path = join(mkdtempSync(join(tmpdir(), "cc-usage-")), "opencode.db")
	const db = new Database(path)
	db.run("CREATE TABLE message (data TEXT, time_created INTEGER)")
	const row = (data: object, at: number) =>
		db.run("INSERT INTO message VALUES (?, ?)", [JSON.stringify(data), at])
	const model = "deepseek/deepseek-v4.1-flash"
	const now = Date.now()
	row({ role: "assistant", modelID: model, tokens: { input: 1 }, cost: 0.5 }, now - 1_000)
	row({ role: "assistant", modelID: model, tokens: { input: 1 }, cost: 1.25 }, now - 2_000)
	row({ role: "assistant", modelID: "other/model", tokens: { input: 1 }, cost: 9 }, now - 1_000)
	row({ role: "user", modelID: model, tokens: { input: 1 }, cost: 3 }, now - 1_000)
	row({ role: "assistant", modelID: model, tokens: { input: 1 }, cost: 5 }, now - 40 * 86_400_000)
	db.close()
	return path
}

describe("loadModelUsage", () => {
	test("sums the window for one model only", () => {
		const path = fixtureDb()
		const week = Date.now() - 7 * 86_400_000
		expect(loadModelUsage("deepseek/deepseek-v4.1-flash", week, path)).toEqual({
			requests: 2,
			cost: 1.75,
		})
		// the vendored id and the bare one share a canonical key
		expect(loadModelUsage("deepseek-v4.1-flash", week, path)?.requests).toBe(2)
	})
	test("returns null without a store", () => {
		expect(loadModelUsage("deepseek/deepseek-v4.1-flash", 0, "/nope/missing.db")).toBeNull()
	})
})

describe("periodStart", () => {
	const now = Date.UTC(2026, 8, 24)
	test("takes the period end one calendar month back", () => {
		expect(new Date(periodStart("2026-09-27", now)).toISOString().slice(0, 10)).toBe("2026-08-27")
	})
	test("falls back to 30 days", () => {
		expect(periodStart(undefined, now)).toBe(now - 30 * 86_400_000)
	})
})
