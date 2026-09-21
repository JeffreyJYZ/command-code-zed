import { describe, expect, test } from "bun:test";
import { KNOWN_MODELS } from "../src/gating";
import { isClaude } from "../src/models";
import { staticSeedModels, toV2Model, type Lane } from "../src/v2";

const claudeLane: Lane = {
	id: "command-code-anthropic",
	name: "Command Code (Anthropic)",
	pkg: "@opencode/ai/providers/anthropic",
};
const openLane: Lane = {
	id: "command-code-openai",
	name: "Command Code (OpenAI)",
	pkg: "@opencode/ai/providers/openai-compatible",
	reasoningField: "reasoning_content",
};

describe("toV2Model", () => {
	test("maps id/name/limits; context 0 falls back to 128k", () => {
		const m = toV2Model({ id: "gpt-5.5", name: "GPT", contextLength: 400_000 }, openLane) as Record<
			string,
			unknown
		>;
		expect(m.id).toBe("gpt-5.5");
		expect(m.modelID).toBe("gpt-5.5");
		expect(m.providerID).toBe("command-code-openai");
		expect(m.name).toBe("GPT");
		expect(m.limit).toEqual({ context: 400_000, output: 32_000 });
		expect(m.status).toBe("active");
		expect(m.enabled).toBe(true);
	});
	test("open lane sets compatibility.reasoningField", () => {
		const m = toV2Model({ id: "deepseek/deepseek-v4-flash", name: "DS", contextLength: 1 }, openLane) as Record<
			string,
			unknown
		>;
		expect(m.compatibility).toEqual({ reasoningField: "reasoning_content" });
	});
	test("claude lane has no compatibility block", () => {
		const m = toV2Model({ id: "claude-sonnet-5", name: "Sonnet", contextLength: 1 }, claudeLane) as Record<
			string,
			unknown
		>;
		expect(m.compatibility).toBeUndefined();
	});
	test("capabilities are text-only tools", () => {
		const m = toV2Model({ id: "x", name: "x", contextLength: 1 }, claudeLane) as Record<string, unknown>;
		expect(m.capabilities).toEqual({ tools: true, input: ["text"], output: ["text"] });
	});
});

describe("staticSeedModels", () => {
	test("splits known models by lane, no context length yet", () => {
		const seed = staticSeedModels();
		const claude = seed["command-code-anthropic"] as Array<Record<string, unknown>>;
		const open = seed["command-code-openai"] as Array<Record<string, unknown>>;
		expect(claude.length + open.length).toBe(KNOWN_MODELS.length);
		for (const m of claude) expect(isClaude(m.id as string)).toBe(true);
		for (const m of open) expect(isClaude(m.id as string)).toBe(false);
		for (const m of [...claude, ...open]) expect((m.limit as { context: number }).context).toBe(128_000);
	});
});
