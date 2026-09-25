import { describe, expect, test } from "bun:test"
import type { LanguageModelV3CallOptions } from "@ai-sdk/provider"
import { createCommandCode, laneOf } from "../src/provider"
import { anthropicReducer, openaiReducer, sseDecoder } from "../src/provider/stream"
import { anthropicBody, openaiBody } from "../src/provider/wire"

const sse = (events: unknown[]) => events.map((e) => `data: ${JSON.stringify(e)}\n\n`).join("")

describe("sseDecoder", () => {
	test("reassembles chunk-split lines and ignores noise", () => {
		const seen: unknown[] = []
		const decode = sseDecoder((e) => seen.push(e))
		decode(': comment\n\ndata: {"a":')
		decode('1}\n\ndata: [DONE]\n\n')
		expect(seen).toEqual([{ a: 1 }])
	})
})

describe("anthropicReducer", () => {
	test("text turn: start/delta/end then finish with usage", () => {
		const reduce = anthropicReducer()
		const parts = [
			...reduce.feed({ type: "message_start" }),
			...reduce.feed({ type: "content_block_start", index: 0, content_block: { type: "text" } }),
			...reduce.feed({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "hi" } }),
			...reduce.feed({ type: "content_block_stop", index: 0 }),
			...reduce.feed({
				type: "message_delta",
				delta: { stop_reason: "end_turn" },
				usage: { input_tokens: 10, output_tokens: 4, cache_read_input_tokens: 6, cache_creation_input_tokens: 1 },
			}),
		]
		expect(parts.map((p) => p.type)).toEqual(["stream-start", "text-start", "text-delta", "text-end", "finish"])
		const finish = parts.at(-1) as { finishReason: { unified: string }; usage: { inputTokens: { noCache: number; cacheRead: number; cacheWrite: number } } }
		expect(finish.finishReason.unified).toBe("stop")
		expect(finish.usage.inputTokens).toMatchObject({ noCache: 3, cacheRead: 6, cacheWrite: 1 })
	})

	test("tool turn: tool_use block and tool-calls finish reason", () => {
		const reduce = anthropicReducer()
		const parts = [
			...reduce.feed({ type: "content_block_start", index: 0, content_block: { type: "tool_use", id: "t1", name: "read" } }),
			...reduce.feed({ type: "content_block_delta", index: 0, delta: { type: "input_json_delta", partial_json: '{"p":1}' } }),
			...reduce.feed({ type: "content_block_stop", index: 0 }),
			...reduce.feed({ type: "message_delta", delta: { stop_reason: "tool_use" }, usage: {} }),
		]
		expect(parts.map((p) => p.type)).toEqual(["tool-input-start", "tool-input-delta", "tool-input-end", "finish"])
		expect((parts.at(-1) as { finishReason: { unified: string } }).finishReason.unified).toBe("tool-calls")
	})
})

describe("openaiReducer", () => {
	test("text turn with reasoning and a trailing usage chunk", () => {
		const reduce = openaiReducer()
		const parts = [
			...reduce.feed({ choices: [{ delta: { reasoning_content: "think" } }] }),
			...reduce.feed({ choices: [{ delta: { content: "hello" } }] }),
			...reduce.feed({ choices: [{ delta: {}, finish_reason: "stop" }] }),
			...reduce.feed({ choices: [], usage: { prompt_tokens: 8, completion_tokens: 2, prompt_tokens_details: { cached_tokens: 3 } } }),
		]
		expect(parts.map((p) => p.type)).toEqual([
			"reasoning-start",
			"reasoning-delta",
			"reasoning-end",
			"text-start",
			"text-delta",
			"text-end",
			"finish",
		])
		const finish = parts.at(-1) as { usage: { inputTokens: { noCache: number; cacheRead: number }; outputTokens: { total: number } } }
		expect(finish.usage.inputTokens).toMatchObject({ noCache: 5, cacheRead: 3 })
		expect(finish.usage.outputTokens.total).toBe(2)
	})

	test("tool call assembled from split argument fragments", () => {
		const reduce = openaiReducer()
		const parts = [
			...reduce.feed({ choices: [{ delta: { tool_calls: [{ index: 0, id: "c1", function: { name: "read", arguments: '{"p"' } }] } }] }),
			...reduce.feed({ choices: [{ delta: { tool_calls: [{ index: 0, function: { arguments: ":1}" } }] } }] }),
			...reduce.feed({ choices: [{ delta: {}, finish_reason: "tool_calls" }] }),
			...reduce.close(),
		]
		const call = parts.find((p) => p.type === "tool-call") as { toolName: string; input: string }
		expect(call.toolName).toBe("read")
		expect(call.input).toBe('{"p":1}')
		expect((parts.at(-1) as { finishReason: { unified: string } }).finishReason.unified).toBe("tool-calls")
	})
})

describe("wire bodies", () => {
	const prompt = [
		{ role: "system", content: "be terse" },
		{ role: "user", content: [{ type: "text", text: "hi" }] },
	] as unknown as LanguageModelV3CallOptions["prompt"]

	test("openai keeps system in messages and asks for usage", () => {
		const body = openaiBody("deepseek/x", prompt, { images: true }) as Record<string, unknown>
		expect((body.messages as unknown[])[0]).toEqual({ role: "system", content: "be terse" })
		expect(body.stream_options).toEqual({ include_usage: true })
		expect(body.max_tokens).toBe(64_000)
	})

	test("anthropic hoists system and caches it", () => {
		const body = anthropicBody("claude-sonnet-5", prompt, { images: true }) as Record<string, unknown>
		expect(body.system).toEqual([{ type: "text", text: "be terse", cache_control: { type: "ephemeral" } }])
		expect((body.messages as unknown[])[0]).toEqual({ role: "user", content: "hi" })
	})

	test("drops a tool call with no matching result", () => {
		const orphan = [
			{ role: "assistant", content: [{ type: "tool-call", toolCallId: "a", toolName: "read", input: {} }] },
		] as unknown as LanguageModelV3CallOptions["prompt"]
		const body = openaiBody("deepseek/x", orphan, { images: true }) as Record<string, unknown>
		expect(body.messages).toEqual([])
	})
})

describe("createCommandCode", () => {
	test("lanes follow the model id", () => {
		expect(laneOf("claude-sonnet-5")).toBe("anthropic")
		expect(laneOf("deepseek/deepseek-v4.1-flash")).toBe("openai")
	})

	test("streams the injected SSE response into v3 parts", async () => {
		const body = sse([
			{ choices: [{ delta: { content: "hi" } }] },
			{ choices: [{ delta: {}, finish_reason: "stop" }] },
			{ choices: [], usage: { prompt_tokens: 1, completion_tokens: 1 } },
		])
		const provider = createCommandCode(
			{
				baseURL: "https://example.test",
				apiKey: "k",
				fetch: (async () => new Response(body, { status: 200, headers: { "content-type": "text/event-stream" } })) as unknown as typeof fetch,
			},
			() => false,
		)
		const { stream } = await provider.languageModel("deepseek/x").doStream({
			prompt: [{ role: "user", content: [{ type: "text", text: "hi" }] }],
		} as LanguageModelV3CallOptions)
		const types: string[] = []
		const reader = stream.getReader()
		for (;;) {
			const { done, value } = await reader.read()
			if (done) break
			types.push((value as { type: string }).type)
		}
		expect(types).toEqual(["text-start", "text-delta", "text-end", "finish"])
	})

	test("surfaces an HTTP failure with the lane and status", async () => {
		const provider = createCommandCode(
			{ baseURL: "https://example.test", fetch: (async () => new Response("nope", { status: 403 })) as unknown as typeof fetch },
			() => false,
		)
		await expect(
			provider.languageModel("claude-sonnet-5").doStream({ prompt: [] } as LanguageModelV3CallOptions),
		).rejects.toThrow(/anthropic 403/)
	})
})
