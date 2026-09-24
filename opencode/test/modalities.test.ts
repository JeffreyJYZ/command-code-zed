import { describe, expect, test } from "bun:test"
import { beyondText, parseModalities } from "../scripts/extract-modalities"
import { inputModalities, supportsImage } from "../src/modalities"
import { toV2Model } from "../src/v2"

// A minified-bundle slice: two records, one vision, one text-only, plus a
// decoy earlier `id` that must not pair with the second record's modalities.
const BUNDLE =
	'spec:{id:"DECOY",label:"decoy"},SONNET:{id:"claude-sonnet-5",inputModalities:["text","image"],provider:FO}' +
	',HAIKU:{id:"claude-haiku-4-5-20251001",inputModalities:["text"],provider:FO}' +
	',OMNI:{id:"qwen/omni",inputModalities:["text","image","audio"],provider:FO}'

describe("parseModalities", () => {
	test("pairs each id with its own modalities", () => {
		expect(parseModalities(BUNDLE)).toEqual({
			"claude-sonnet-5": ["text", "image"],
			"claude-haiku-4-5-20251001": ["text"],
			"qwen/omni": ["text", "image", "audio"],
		})
	})
	test("drops text-only records from the generated slice", () => {
		expect(Object.keys(beyondText(parseModalities(BUNDLE)))).toEqual([
			"claude-sonnet-5",
			"qwen/omni",
		])
	})
})

describe("generated modality table", () => {
	test("flags known vision models and defaults the rest to text", () => {
		expect(supportsImage("claude-sonnet-5")).toBe(true)
		expect(supportsImage("deepseek/deepseek-v4-flash-vision-exp")).toBe(true)
		expect(inputModalities("definitely/not-a-model")).toEqual(["text"])
		expect(supportsImage("definitely/not-a-model")).toBe(false)
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
