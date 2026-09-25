import { describe, expect, test } from "bun:test"
import { mergeCatalog, parseModelsMd, parseModalities } from "../scripts/extract-catalog"
import { CATALOG_VERSION, inputModalities, isReasoningModel, modelCost, supportsImage } from "../src/catalog"
import { toV2Model } from "../src/v2"

// A docs-table slice: one priced vision row, one priced text row, one row
// without efforts, one `—` row, and a Claude row with cache-write.
const MD = [
	"| Id | Name | Context | Efforts | $/1M in/out · cache read | Min plan | Best for |",
	"|---|---|---|---|---|---|---|",
	'| `claude-sonnet-5` | Claude Sonnet 5 | 1M | low, max | $2/$10 · cache $0.2 (write $2.5) | Pro and above | good |',
	'| `deepseek/x` | X | 256K | high, max | $0.15/$0.6 · cache $0.003 | Go and above | fast |',
	'| `z/z` | Z | 1.05M | — | $0/$0 · cache $0 | Go and above | free |',
	"| `bad` | Bad | 1M | low | nope | Go | x |",
].join("\n")

// A minified-bundle slice: the decoy `id` must not pair with a later list.
const BUNDLE =
	'spec:{id:"DECOY",label:"decoy"},SONNET:{id:"claude-sonnet-5",inputModalities:["text","image"],provider:FO}' +
	',XX:{id:"deepseek/x",inputModalities:["text"],provider:FO}'

describe("parseModelsMd", () => {
	test("parses rates, context, efforts and cache-write", () => {
		const rows = parseModelsMd(MD)
		expect(rows["claude-sonnet-5"]).toEqual({
			name: "Claude Sonnet 5",
			context: 1_000_000,
			efforts: ["low", "max"],
			cost: { input: 2, output: 10, cacheRead: 0.2, cacheWrite: 2.5 },
		})
		expect(rows["deepseek/x"]?.cost).toEqual({ input: 0.15, output: 0.6, cacheRead: 0.003, cacheWrite: 0 })
		expect(rows["z/z"]?.efforts).toBeNull()
		expect(rows["z/z"]?.context).toBe(1_050_000)
		expect(rows["bad"]).toBeUndefined()
	})
})

describe("parseModalities", () => {
	test("pairs each id with its own modalities", () => {
		expect(parseModalities(BUNDLE)).toEqual({
			"claude-sonnet-5": ["text", "image"],
			"deepseek/x": ["text"],
		})
	})
})

describe("mergeCatalog", () => {
	test("joins pricing with modalities, defaulting to text", () => {
		const catalog = mergeCatalog(parseModelsMd(MD), parseModalities(BUNDLE))
		expect(catalog["claude-sonnet-5"]?.modalities).toEqual(["text", "image"])
		expect(catalog["deepseek/x"]?.modalities).toEqual(["text"])
	})
})

describe("generated catalog", () => {
	test("is version-stamped", () => {
		expect(typeof CATALOG_VERSION).toBe("string")
		expect(CATALOG_VERSION.length).toBeGreaterThan(0)
	})
	test("flags vision, prices and reasoning from the table", () => {
		expect(supportsImage("claude-sonnet-5")).toBe(true)
		expect(supportsImage("deepseek/deepseek-v4-flash-vision-exp")).toBe(true)
		expect(inputModalities("definitely/not-a-model")).toEqual(["text"])
		expect(modelCost("deepseek/deepseek-v4.1-flash")).toEqual({
			input: 0.15,
			output: 0.6,
			cacheRead: 0.003,
			cacheWrite: 0,
		})
		expect(modelCost("definitely/not-a-model")).toBeUndefined()
		expect(isReasoningModel("claude-sonnet-5")).toBe(true)
		expect(isReasoningModel("definitely/not-a-model")).toBe(false)
	})
})

describe("v2 model capabilities", () => {
	const lane = {
		id: "command-code-anthropic" as const,
		name: "Command Code (Anthropic)",
		pkg: "@opencode/ai/providers/anthropic",
	}
	test("advertises image input for a vision model", () => {
		const info = toV2Model({ id: "claude-sonnet-5", name: "Claude Sonnet 5", contextLength: 1e6 }, lane) as {
			capabilities: { input: string[] }
		}
		expect(info.capabilities.input).toEqual(["text", "image"])
	})
	test("stays text-only for an unlisted model", () => {
		const info = toV2Model({ id: "unknown/model", name: "?", contextLength: 0 }, lane) as {
			capabilities: { input: string[] }
		}
		expect(info.capabilities.input).toEqual(["text"])
	})
})
