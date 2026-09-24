import { describe, expect, test } from "bun:test"
import {
	count,
	modelKey,
	tierFor,
	modelRows,
	money,
	rate,
	until,
	usageRows,
} from "../src/sidebar/rows"

describe("formatting", () => {
	test("money trims whole numbers but keeps cents", () => {
		expect(money(60)).toBe("$60")
		expect(money(12.5)).toBe("$12.50")
		expect(money(0.125)).toBe("$0.13")
	})
	test("rate keeps sub-cent precision", () => {
		expect(rate(0)).toBe("$0")
		expect(rate(0.0036)).toBe("$0.0036")
		expect(rate(0.6)).toBe("$0.6")
	})
	test("count abbreviates", () => {
		expect(count(940)).toBe("940")
		expect(count(6_296)).toBe("6.3K")
		expect(count(2_400_000)).toBe("2.4M")
	})
	test("until renders a short countdown", () => {
		const now = 1_000_000_000_000
		expect(until(now + 30_000, now)).toBe("<1m")
		expect(until(now + 5 * 60_000, now)).toBe("5m")
		expect(until(now + 3 * 3_600_000 + 12 * 60_000, now)).toBe("3h 12m")
		expect(until(now + 44 * 3_600_000, now)).toBe("1d 20h")
		expect(until(undefined, now)).toBe("")
	})
})

describe("usageRows", () => {
	const usage = {
		plan: "GOAT",
		monthlyCap: 70,
		monthlyCredits: 19.44,
		fiveHour: { cap: 14, used: 1.17, resetAt: 1_000_000_000_000 },
		weekly: { cap: 35, used: 12.43, resetAt: 1_000_000_000_000 },
		requests: 6_296,
		cost: 46.31,
	}
	test("renders plan, monthly, windows and period", () => {
		const rows = usageRows(usage, 1_000_000_000_000 - 5 * 60_000)
		expect(rows[0]).toEqual(["Plan", "GOAT · $70/mo credits"])
		expect(rows[1]).toEqual(["Monthly", "$50.56 / $70 (72%)"])
		expect(rows[2]?.[0]).toBe("5-hour")
		expect(rows[2]?.[1]).toContain("(8%)")
		expect(rows[2]?.[1]).toContain("resets 5m")
		expect(rows[3]?.[0]).toBe("Weekly")
		expect(rows[4]).toEqual(["Period", "6.3K requests · $46.31"])
	})
	test("empty input yields no rows", () => {
		expect(usageRows(undefined)).toEqual([])
	})
})

describe("modelRows", () => {
	test("renders tier, allowance, rates and benchmarks", () => {
		const rows = modelRows({
			key: "deepseekv41flash",
			name: "DeepSeek V4.1 Flash",
			tier: "opensource",
			allowance: 60,
			rates: { input: 0.15, output: 0.6, cacheRead: 0.003 },
			intelligence: 39.5,
			tps: 247,
		})
		expect(rows[0]).toEqual(["Model", "DeepSeek V4.1 Flash"])
		expect(rows[1]).toEqual(["Tier", "open source"])
		expect(rows[2]).toEqual(["Allowance", "$60/mo"])
		expect(rows[3]).toEqual(["Rates", "$0.15/$0.6 in/out"])
		expect(rows[4]).toEqual(["Cache read", "$0.003"])
		expect(rows[5]).toEqual(["Intelligence", "39.5"])
		expect(rows[6]).toEqual(["Tok/s", "247"])
	})
	test("puts period usage under the model name when known", () => {
		const rows = modelRows(
			{ key: "deepseekv41flash", name: "DeepSeek V4.1 Flash" },
			{ requests: 1_234, cost: 8.4 },
		)
		expect(rows[0]).toEqual(["Model", "DeepSeek V4.1 Flash"])
		expect(rows[1]).toEqual(["Usage", "1.2K req · $8.40"])
	})
	test("omits spend when the harness priced it at zero (subscription)", () => {
		const rows = modelRows(
			{ key: "deepseekv41flash", name: "DeepSeek V4.1 Flash" },
			{ requests: 3_110, cost: 0 },
		)
		expect(rows[1]).toEqual(["Usage", "3.1K req"])
	})
	test("missing meta yields no rows", () => {
		expect(modelRows(undefined)).toEqual([])
	})
})

describe("tierFor", () => {
	test("reads the gating categories by model id", () => {
		expect(tierFor("deepseek/deepseek-v4-flash")).toBe("opensource")
		expect(tierFor("claude-sonnet-5")).toBe("premium")
		expect(tierFor("nope/nothing")).toBeUndefined()
	})
})

describe("modelKey", () => {
	test("mirrors mpc's normalization", () => {
		expect(modelKey("deepseek/deepseek-v4.1-flash")).toBe("deepseekv41flash")
		expect(modelKey("DeepSeek V4 Flash (latest)")).toBe("deepseekv4flash")
		expect(modelKey("zai-org/GLM-5.2-Fast")).toBe("glm52fast")
	})
})
