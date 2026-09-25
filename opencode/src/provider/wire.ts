// Wire bodies for Command Code's provider API: Anthropic Messages for the
// Claude lane, OpenAI Chat Completions for everything else. Pure conversion —
// no IO — so fixture tests pin the exact bytes. Prompt parts are typed
// structurally (type-only import) so the published bundle has no @ai-sdk
// runtime dependency; only the returned object shape matters to the host.
import type {
	LanguageModelV3Message,
	LanguageModelV3Prompt,
	LanguageModelV3ToolResultPart,
} from "@ai-sdk/provider"

export type WireLane = "anthropic" | "openai"

/** The subset of tools we serialize (V3 FunctionTool already matches). */
export interface WireTool {
	name: string
	description?: string
	inputSchema: unknown
}

export interface WireOptions {
	maxOutputTokens?: number
	temperature?: number
	tools?: WireTool[]
	/** Wire extension both gates honour (`reasoning_effort`). */
	reasoningEffort?: string
	/** Whether the model advertises image input (catalog-derived). */
	images: boolean
}

const DEFAULT_MAX_TOKENS = 64_000

/** Cap the caller's value like upstream: never above the provider ceiling. */
function capTokens(value: number | undefined): number {
	const n = typeof value === "number" && Number.isFinite(value) ? value : DEFAULT_MAX_TOKENS
	return Math.min(n, DEFAULT_MAX_TOKENS)
}

function textOf(part: unknown): string {
	const p = part as { text?: unknown }
	return typeof p.text === "string" ? p.text : ""
}

/** Tool results carry a nested value shape; flatten it the way the API expects. */
function resultText(part: LanguageModelV3ToolResultPart): string {
	const output = (part as { output?: unknown }).output ?? (part as { result?: unknown }).result
	if (typeof output === "string") return output
	if (output && typeof output === "object") {
		const o = output as { type?: string; value?: unknown; reason?: unknown }
		if (typeof o.value === "string") return o.value
		if (o.value !== undefined) return JSON.stringify(o.value)
		if (typeof o.reason === "string") return o.reason
	}
	return JSON.stringify(output ?? "")
}

/** `data:<mime>;base64,<b64>` (or a raw URL) for a file part. */
function imageUrl(part: unknown): string | undefined {
	const p = part as { data?: unknown; mediaType?: unknown; mimeType?: unknown }
	const mime = (p.mediaType ?? p.mimeType ?? "application/octet-stream") as string
	const data = p.data
	if (typeof data === "string") return data.startsWith("data:") ? data : `data:${mime};base64,${data}`
	if (data instanceof Uint8Array) return `data:${mime};base64,${Buffer.from(data).toString("base64")}`
	if (data instanceof URL) return data.href
	return undefined
}

/** Only the ids that both a call and its result are present for: an orphan
 * either side is rejected by both wires. */
function pairedToolCallIds(prompt: LanguageModelV3Prompt): Set<string> {
	const calls = new Set<string>()
	const results = new Set<string>()
	for (const message of prompt) {
		for (const part of message.content as unknown as Array<Record<string, unknown>>) {
			if (part.type === "tool-call" && typeof part.toolCallId === "string") calls.add(part.toolCallId)
			if (part.type === "tool-result" && typeof part.toolCallId === "string") results.add(part.toolCallId)
		}
	}
	return new Set([...calls].filter((id) => results.has(id)))
}

function systemText(prompt: LanguageModelV3Prompt): string {
	return prompt
		.filter((m) => m.role === "system")
		.map((m) => (m as { content: string }).content)
		.join("\n")
}

function anthropicUserContent(content: unknown, images: boolean): unknown {
	if (typeof content === "string") return content
	const parts = (content ?? []) as unknown as Array<Record<string, unknown>>
	const blocks: unknown[] = []
	for (const part of parts) {
		if (part.type === "text") blocks.push({ type: "text", text: textOf(part) })
		else if (part.type === "file" && images) {
			const url = imageUrl(part)
			if (url?.startsWith("data:")) {
				const [head, b64] = url.slice(5).split(";base64,")
				blocks.push({ type: "image", source: { type: "base64", media_type: head, data: b64 } })
			}
		}
	}
	// Anthropic accepts a bare string for a single text part; keep that shape.
	if (blocks.length === 1 && (blocks[0] as { type: string }).type === "text") {
		return (blocks[0] as { text: string }).text
	}
	return blocks
}

function openaiUserContent(content: unknown, images: boolean): unknown {
	if (typeof content === "string") return content
	const parts = (content ?? []) as unknown as Array<Record<string, unknown>>
	const blocks: unknown[] = []
	for (const part of parts) {
		if (part.type === "text") blocks.push({ type: "text", text: textOf(part) })
		else if (part.type === "file" && images) {
			const url = imageUrl(part)
			if (url) blocks.push({ type: "image_url", image_url: { url } })
		}
	}
	if (blocks.every((b) => (b as { type: string }).type === "text")) {
		return blocks.map((b) => (b as { text: string }).text).join("\n")
	}
	return blocks
}

/** Anthropic body: POST /provider/v1/messages. */
export function anthropicBody(
	model: string,
	prompt: LanguageModelV3Prompt,
	options: WireOptions,
): Record<string, unknown> {
	const paired = pairedToolCallIds(prompt)
	const messages: unknown[] = []
	for (const message of prompt as LanguageModelV3Message[]) {
		if (message.role === "system") continue
		if (message.role === "user") {
			messages.push({ role: "user", content: anthropicUserContent(message.content, options.images) })
			continue
		}
		if (message.role === "assistant") {
			const content: unknown[] = []
			for (const part of message.content as unknown as Array<Record<string, unknown>>) {
				if (part.type === "text") {
					const text = textOf(part)
					if (text) content.push({ type: "text", text })
				} else if (part.type === "tool-call" && paired.has(String(part.toolCallId))) {
					content.push({ type: "tool_use", id: part.toolCallId, name: part.toolName, input: part.input ?? {} })
				}
			}
			if (content.length > 0) messages.push({ role: "assistant", content })
			continue
		}
		// tool results become user turns of tool_result blocks
		for (const part of message.content as unknown as Array<Record<string, unknown>>) {
			if (part.type !== "tool-result" || !paired.has(String(part.toolCallId))) continue
			messages.push({
				role: "user",
				content: [
					{
						type: "tool_result",
						tool_use_id: part.toolCallId,
						content: resultText(part as unknown as LanguageModelV3ToolResultPart),
					},
				],
			})
		}
	}
	const system = systemText(prompt)
	const body: Record<string, unknown> = { model, stream: true, messages, max_tokens: capTokens(options.maxOutputTokens) }
	// One ephemeral cache breakpoint on the stable system prefix.
	if (system) body.system = [{ type: "text", text: system, cache_control: { type: "ephemeral" } }]
	if (options.tools?.length) {
		body.tools = options.tools.map((t) => ({ name: t.name, description: t.description, input_schema: t.inputSchema ?? {} }))
	}
	if (options.temperature !== undefined) body.temperature = options.temperature
	if (options.reasoningEffort) body.reasoning_effort = options.reasoningEffort
	return body
}

/** OpenAI body: POST /provider/v1/chat/completions. */
export function openaiBody(
	model: string,
	prompt: LanguageModelV3Prompt,
	options: WireOptions,
): Record<string, unknown> {
	const paired = pairedToolCallIds(prompt)
	const messages: unknown[] = []
	const system = systemText(prompt)
	if (system) messages.push({ role: "system", content: system })
	for (const message of prompt as LanguageModelV3Message[]) {
		if (message.role === "system") continue
		if (message.role === "user") {
			messages.push({ role: "user", content: openaiUserContent(message.content, options.images) })
			continue
		}
		if (message.role === "assistant") {
			const calls = (message.content as unknown as Array<Record<string, unknown>>).filter(
				(p) => p.type === "tool-call" && paired.has(String(p.toolCallId)),
			)
			const texts = (message.content as unknown as Array<Record<string, unknown>>)
				.filter((p) => p.type === "text")
				.map(textOf)
				.filter(Boolean)
			const reasoning = (message.content as unknown as Array<Record<string, unknown>>)
				.filter((p) => p.type === "reasoning")
				.map(textOf)
				.join("")
			if (calls.length > 0) {
				messages.push({
					role: "assistant",
					...(reasoning ? { reasoning_content: reasoning } : {}),
					content: texts.length ? texts.join("\n") : null,
					tool_calls: calls.map((c) => ({
						id: c.toolCallId,
						type: "function",
						function: { name: c.toolName, arguments: JSON.stringify(c.input ?? {}) },
					})),
				})
			} else if (texts.length > 0) {
				messages.push({ role: "assistant", ...(reasoning ? { reasoning_content: reasoning } : {}), content: texts.join("\n") })
			}
			continue
		}
		for (const part of message.content as unknown as Array<Record<string, unknown>>) {
			if (part.type !== "tool-result" || !paired.has(String(part.toolCallId))) continue
			messages.push({
				role: "tool",
				tool_call_id: part.toolCallId,
				content: resultText(part as unknown as LanguageModelV3ToolResultPart),
			})
		}
	}
	const body: Record<string, unknown> = { model, stream: true, messages, max_tokens: capTokens(options.maxOutputTokens) }
	if (options.tools?.length) {
		body.tools = options.tools.map((t) => ({
			type: "function",
			function: { name: t.name, description: t.description, parameters: t.inputSchema ?? {} },
		}))
	}
	if (options.temperature !== undefined) body.temperature = options.temperature
	if (options.reasoningEffort) body.reasoning_effort = options.reasoningEffort
	body.stream_options = { include_usage: true }
	return body
}

export function requestBody(
	lane: WireLane,
	model: string,
	prompt: LanguageModelV3Prompt,
	options: WireOptions,
): Record<string, unknown> {
	return lane === "anthropic" ? anthropicBody(model, prompt, options) : openaiBody(model, prompt, options)
}
