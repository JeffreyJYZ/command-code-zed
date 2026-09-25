// SSE → AI SDK v3 stream parts, for both Command Code wires.
//
// Anthropic Messages events (`content_block_*`, `message_delta`, `message_stop`)
// and OpenAI Chat Completions chunks (`choices[0].delta`, trailing usage-only
// chunk) both reduce to the same part vocabulary: text, reasoning, tool calls,
// usage and finish. Pure state machines — the fixture tests drive them with
// recorded event lines, so no network is needed to pin behaviour.
import type { LanguageModelV3FinishReason, LanguageModelV3StreamPart, LanguageModelV3Usage } from "@ai-sdk/provider"

type Part = LanguageModelV3StreamPart

/** Split an SSE byte stream into JSON events, tolerating partial chunks. */
export function sseDecoder(onEvent: (event: Record<string, unknown>) => void): (text: string) => void {
	let buffer = ""
	return (text: string) => {
		buffer += text
		for (;;) {
			const nl = buffer.indexOf("\n")
			if (nl < 0) return
			const line = buffer.slice(0, nl).replace(/\r$/, "")
			buffer = buffer.slice(nl + 1)
			const trimmed = line.trim()
			if (!trimmed || trimmed.startsWith(":") || trimmed.startsWith("event:")) continue
			const payload = trimmed.startsWith("data:") ? trimmed.slice(5).trim() : trimmed
			if (!payload || payload === "[DONE]") continue
			try {
				const parsed = JSON.parse(payload)
				if (parsed && typeof parsed === "object") onEvent(parsed as Record<string, unknown>)
			} catch {
				// keep going: one malformed line must not kill the turn
			}
		}
	}
}

const text = (v: unknown): string => (typeof v === "string" ? v : "")
const num = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : 0)
const rec = (v: unknown): Record<string, unknown> => (v && typeof v === "object" ? (v as Record<string, unknown>) : {})

const TOOL_FINISH = new Set(["tool_use", "tool_calls", "tool-call", "function_call"])
const LENGTH_FINISH = new Set(["length", "max_tokens", "max_output_tokens", "model_context_window_exceeded"])

/** Wire stop reason → unified v3 finish reason; unknown reasons completed. */
export function finishReason(raw: string | undefined): LanguageModelV3FinishReason {
	const value = (raw ?? "unknown").toLowerCase()
	if (TOOL_FINISH.has(value)) return { unified: "tool-calls", raw }
	if (LENGTH_FINISH.has(value)) return { unified: "length", raw }
	if (value === "error") return { unified: "error", raw }
	return { unified: "stop", raw }
}

function usageFrom(
	total: number,
	noCache: number,
	cacheRead: number,
	cacheWrite: number,
	output: number,
	reasoning: number,
): LanguageModelV3Usage {
	return {
		inputTokens: { total, noCache, cacheRead, cacheWrite },
		outputTokens: { total: output, text: Math.max(0, output - reasoning), reasoning },
	}
}

/** Anthropic `message_start`/`message_delta` usage → v3 usage. */
export function anthropicUsage(raw: Record<string, unknown>): LanguageModelV3Usage {
	const total = num(raw.input_tokens)
	const cacheRead = num(raw.cache_read_input_tokens)
	const cacheWrite = num(raw.cache_creation_input_tokens)
	const output = num(raw.output_tokens)
	return usageFrom(total, Math.max(0, total - cacheRead - cacheWrite), cacheRead, cacheWrite, output, 0)
}

/** OpenAI usage → v3 usage (`prompt_tokens_details.cached_tokens` is cache read). */
export function openaiUsage(raw: Record<string, unknown>): LanguageModelV3Usage {
	const total = num(raw.prompt_tokens)
	const cached = num(rec(raw.prompt_tokens_details).cached_tokens)
	const output = num(raw.completion_tokens)
	const reasoning = num(rec(raw.completion_tokens_details).reasoning_tokens)
	return usageFrom(total, Math.max(0, total - cached), cached, 0, output, reasoning)
}

export interface Reducer {
	/** Feed one parsed SSE event; returns the parts it produced. */
	feed(event: Record<string, unknown>): Part[]
	/** Close open parts when the transport ends (abort/error). */
	close(): Part[]
}

/** Anthropic Messages reducer. */
export function anthropicReducer(): Reducer {
	const blocks = new Map<number, { type: string; id: string }>()
	let finished = false
	let warningsEmitted = false
	let last: LanguageModelV3FinishReason = { unified: "stop", raw: "unknown" }

	const open = (index: number, kind: "text" | "thinking"): Part[] => {
		const id = `${kind === "text" ? "text" : "reasoning"}-${index}`
		blocks.set(index, { type: kind, id })
		return [{ type: kind === "text" ? "text-start" : "reasoning-start", id }]
	}

	return {
		feed(event) {
			const type = text(event.type)
			if (type === "message_start") {
				if (warningsEmitted) return []
				warningsEmitted = true
				return [{ type: "stream-start", warnings: [] }]
			}
			if (type === "content_block_start") {
				const index = num(event.index)
				const block = rec(event.content_block)
				const blockType = text(block.type)
				if (blockType === "text") return open(index, "text")
				if (blockType === "thinking") return open(index, "thinking")
				if (blockType === "tool_use") {
					const id = text(block.id) || `tool-${index}`
					blocks.set(index, { type: "tool_use", id })
					return [{ type: "tool-input-start", id, toolName: text(block.name) }]
				}
				blocks.set(index, { type: "other", id: `block-${index}` })
				return []
			}
			if (type === "content_block_delta") {
				const index = num(event.index)
				const delta = rec(event.delta)
				const block = blocks.get(index)
				const textDelta = text(delta.text)
				if (textDelta) {
					const parts = block?.type === "text" ? [] : open(index, "text")
					const id = blocks.get(index)?.id ?? `text-${index}`
					return [...parts, { type: "text-delta", id, delta: textDelta }]
				}
				const thinking = text(delta.thinking)
				if (thinking) {
					const parts = block?.type === "thinking" ? [] : open(index, "thinking")
					const id = blocks.get(index)?.id ?? `reasoning-${index}`
					return [...parts, { type: "reasoning-delta", id, delta: thinking }]
				}
				const partial = text(delta.partial_json)
				if (partial && block?.type === "tool_use") {
					return [{ type: "tool-input-delta", id: block.id, delta: partial }]
				}
				return []
			}
			if (type === "content_block_stop") {
				const index = num(event.index)
				const block = blocks.get(index)
				if (!block) return []
				blocks.delete(index)
				if (block.type === "text") return [{ type: "text-end", id: block.id }]
				if (block.type === "thinking") return [{ type: "reasoning-end", id: block.id }]
				if (block.type === "tool_use") return [{ type: "tool-input-end", id: block.id }]
				return []
			}
			if (type === "message_delta") {
				const delta = rec(event.delta)
				last = finishReason(text(delta.stop_reason) || undefined)
				if (text(delta.stop_reason) === "pause_turn") last = { unified: "stop", raw: "pause_turn" }
				if (finished) return []
				finished = true
				return [{ type: "finish", finishReason: last, usage: anthropicUsage(rec(event.usage)) }]
			}
			if (type === "message_stop" && !finished) {
				finished = true
				return [{ type: "finish", finishReason: last, usage: usageFrom(0, 0, 0, 0, 0, 0) }]
			}
			return []
		},
		close() {
			const parts: Part[] = []
			for (const block of blocks.values()) {
				if (block.type === "text") parts.push({ type: "text-end", id: block.id })
				if (block.type === "thinking") parts.push({ type: "reasoning-end", id: block.id })
				if (block.type === "tool_use") parts.push({ type: "tool-input-end", id: block.id })
			}
			blocks.clear()
			return parts
		},
	}
}

interface ToolBuffer {
	id: string
	name: string
	input: string
	opened: boolean
	closed: boolean
}

/** OpenAI Chat Completions reducer. */
export function openaiReducer(): Reducer {
	const tools = new Map<number, ToolBuffer>()
	let textId: string | undefined
	let reasoningId: string | undefined
	let pending: LanguageModelV3FinishReason | undefined
	let usage: LanguageModelV3Usage | undefined
	let finished = false

	const closeText = (): Part[] => {
		if (!textId) return []
		const id = textId
		textId = undefined
		return [{ type: "text-end", id }]
	}
	const closeReasoning = (): Part[] => {
		if (!reasoningId) return []
		const id = reasoningId
		reasoningId = undefined
		return [{ type: "reasoning-end", id }]
	}
	const completeIfParsed = (buffer: ToolBuffer): Part[] => {
		if (!buffer.opened || buffer.closed) return []
		try {
			JSON.parse(buffer.input)
		} catch {
			return [] // still accumulating fragments
		}
		buffer.closed = true
		return [
			{ type: "tool-input-end", id: buffer.id },
			{ type: "tool-call", toolCallId: buffer.id, toolName: buffer.name, input: buffer.input },
		]
	}
	const flushTools = (): Part[] => {
		const parts: Part[] = []
		for (const [index, buffer] of tools) {
			if (buffer.opened && !buffer.closed) {
				parts.push({ type: "tool-input-end", id: buffer.id })
				parts.push({ type: "tool-call", toolCallId: buffer.id, toolName: buffer.name, input: buffer.input })
				buffer.closed = true
			}
			tools.delete(index)
		}
		return parts
	}

	return {
		feed(event) {
			if (text(event.type) === "error" || event.error !== undefined) {
				const message = text(rec(event.error).message) || text(event.message) || "provider stream error"
				return [{ type: "error", error: new Error(message) }]
			}
			const parts: Part[] = []
			const choices = Array.isArray(event.choices) ? event.choices : []
			const choice = rec(choices[0])
			const delta = rec(choice.delta)

			const reasoning = text(delta.reasoning) || text(delta.reasoning_content)
			if (reasoning) {
				if (!reasoningId) {
					parts.push(...closeText())
					reasoningId = "reasoning-0"
					parts.push({ type: "reasoning-start", id: reasoningId })
				}
				parts.push({ type: "reasoning-delta", id: reasoningId, delta: reasoning })
			}
			const content = text(delta.content)
			if (content) {
				if (reasoningId) parts.push(...closeReasoning())
				if (!textId) {
					textId = "text-0"
					parts.push({ type: "text-start", id: textId })
				}
				parts.push({ type: "text-delta", id: textId, delta: content })
			}
			const calls = Array.isArray(delta.tool_calls) ? delta.tool_calls : []
			if (calls.length > 0) {
				parts.push(...closeReasoning(), ...closeText())
				for (const raw of calls) {
					const call = rec(raw)
					const fn = rec(call.function)
					const index = typeof call.index === "number" ? call.index : tools.size
					let buffer = tools.get(index)
					if (!buffer) {
						buffer = { id: text(call.id) || `tool-${index}`, name: text(fn.name), input: "", opened: false, closed: false }
						tools.set(index, buffer)
					}
					if (text(call.id)) buffer.id = text(call.id)
					if (text(fn.name)) buffer.name = text(fn.name)
					const args = text(fn.arguments)
					if (!buffer.opened && buffer.name) {
						buffer.opened = true
						parts.push({ type: "tool-input-start", id: buffer.id, toolName: buffer.name })
						if (buffer.input) parts.push({ type: "tool-input-delta", id: buffer.id, delta: buffer.input })
					} else if (args && !buffer.closed) {
						parts.push({ type: "tool-input-delta", id: buffer.id, delta: args })
					}
					if (!buffer.closed) {
						buffer.input += args
						parts.push(...completeIfParsed(buffer))
					}
				}
			}

			const rawFinish =
				text(choice.finish_reason) || text(choice.finishReason) || text(event.finish_reason) || text(event.finishReason)
			if (rawFinish) pending = finishReason(rawFinish)
			if (event.usage !== undefined && event.usage !== null) usage = openaiUsage(rec(event.usage))
			// Hold the finish until the trailing usage-only chunk (choices: []) or
			// the run's end: with stream_options.include_usage the real tokens ride
			// that last chunk, and emitting on the finish_reason chunk would report
			// zeros. A provider that never sends usage gets its finish from close().
			if (pending && !finished && (usage !== undefined || choices.length === 0)) {
				finished = true
				parts.push(...closeReasoning(), ...closeText(), ...flushTools())
				parts.push({ type: "finish", finishReason: pending, usage: usage ?? usageFrom(0, 0, 0, 0, 0, 0) })
			}
			return parts
		},
		close() {
			const parts = [...closeReasoning(), ...closeText(), ...flushTools()]
			if (pending && !finished) {
				finished = true
				parts.push({ type: "finish", finishReason: pending, usage: usage ?? usageFrom(0, 0, 0, 0, 0, 0) })
			}
			return parts
		},
	}
}
