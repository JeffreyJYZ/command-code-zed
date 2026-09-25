// The plugin's own AI SDK v3 LanguageModel for Command Code's provider API.
//
// Why own it: the lanes used to point `provider.package` at opencode's internal
// `@opencode/ai/providers/*`, which ties us to a package opencode may rename or
// reshape, and gave us no control over pricing, image parts or reasoning
// replay. Here the two wires stay a pure conversion (`wire.ts`) plus a state
// machine (`stream.ts`), and this file is only transport: build body, POST,
// decode SSE, yield v3 parts.
//
// Deliberately not (yet) ported from the reference implementation: retry
// ladders, pause_turn continuation, and the legacy `/alpha/generate` fallback
// for Go-plan accounts that the provider API gates. Those are follow-ups; this
// is the tracer path both hosts can use today.
import type { LanguageModelV3, LanguageModelV3CallOptions, LanguageModelV3StreamResult, LanguageModelV3Usage } from "@ai-sdk/provider"
import { anthropicReducer, openaiReducer, sseDecoder, type Reducer } from "./stream"
import { requestBody, type WireLane, type WireTool } from "./wire"

export const DEFAULT_BASE_URL = "https://api.commandcode.ai"

export interface CommandCodeProviderOptions {
	/** Bearer key; the v2 host passes the resolved connection credential here. */
	apiKey?: string
	baseURL?: string
	/** Extra headers (host-supplied), merged last. */
	headers?: Record<string, string>
	/** Reasoning effort the host wants (`providerOptions.commandcode.reasoning`). */
	reasoningEffort?: string
	/** Injected for tests. */
	fetch?: typeof fetch
}

export interface CommandCodeProvider {
	languageModel(modelId: string): LanguageModelV3
}

/** Claude models use the Anthropic wire; everything else chat completions. */
export function laneOf(modelId: string): WireLane {
	const bare = modelId.includes("/") ? modelId.slice(modelId.lastIndexOf("/") + 1) : modelId
	return bare.toLowerCase().startsWith("claude") ? "anthropic" : "openai"
}

function endpoint(lane: WireLane): string {
	return lane === "anthropic" ? "/provider/v1/messages" : "/provider/v1/chat/completions"
}

/** `providerOptions.commandcode.reasoning` (or a top-level `reasoning`). */
function effortFrom(options: LanguageModelV3CallOptions): string | undefined {
	const providerOptions = options.providerOptions ?? {}
	const top = providerOptions.reasoning ?? providerOptions.reasoningEffort
	if (typeof top === "string") return top
	const namespaced = providerOptions.commandcode
	if (namespaced && typeof namespaced === "object") {
		const value = (namespaced as Record<string, unknown>).reasoning ?? (namespaced as Record<string, unknown>).reasoningEffort
		if (typeof value === "string") return value
	}
	return undefined
}

function toolsOf(options: LanguageModelV3CallOptions): WireTool[] | undefined {
	const tools = options.tools ?? []
	if (tools.length === 0) return undefined
	return tools.map((tool) => ({
		name: tool.name,
		description: "description" in tool ? tool.description : undefined,
		inputSchema: (tool as { inputSchema?: unknown }).inputSchema ?? {},
	}))
}

/** True when the model advertises image input; the caller passes the table in. */
export type ImagesFor = (modelId: string) => boolean

export function createCommandCode(
	options: CommandCodeProviderOptions = {},
	images: ImagesFor = () => false,
): CommandCodeProvider {
	const baseURL = (options.baseURL ?? DEFAULT_BASE_URL).replace(/\/+$/, "")
	const doFetch = options.fetch ?? fetch

	function model(modelId: string): LanguageModelV3 {
		const lane = laneOf(modelId)
		return {
			specificationVersion: "v3",
			provider: "command-code",
			modelId,
			supportedUrls: {},
			async doStream(callOptions: LanguageModelV3CallOptions): Promise<LanguageModelV3StreamResult> {
				const body = requestBody(lane, modelId, callOptions.prompt, {
					maxOutputTokens: callOptions.maxOutputTokens,
					temperature: callOptions.temperature,
					tools: toolsOf(callOptions),
					reasoningEffort: effortFrom(callOptions) ?? options.reasoningEffort,
					images: images(modelId),
				})
				const response = await doFetch(`${baseURL}${endpoint(lane)}`, {
					method: "POST",
					headers: {
						"Content-Type": "application/json",
						Authorization: `Bearer ${options.apiKey ?? ""}`,
						...(options.headers ?? {}),
						...(callOptions.headers ?? {}),
					},
					body: JSON.stringify(body),
					signal: callOptions.abortSignal,
				})
				if (!response.ok || !response.body) {
					const detail = await response.text().catch(() => "")
					throw new Error(`command-code ${lane} ${response.status}: ${detail.slice(0, 300)}`)
				}

				const reducer: Reducer = lane === "anthropic" ? anthropicReducer() : openaiReducer()
				const stream = new ReadableStream({
					async start(controller) {
						const reader = response.body!.getReader()
						const decoder = new TextDecoder()
						const emit = (parts: ReturnType<Reducer["feed"]>) => {
							for (const part of parts) controller.enqueue(part)
						}
						const decode = sseDecoder((event) => emit(reducer.feed(event)))
						try {
							for (;;) {
								const { done, value } = await reader.read()
								if (done) break
								decode(decoder.decode(value, { stream: true }))
							}
							emit(reducer.close())
							controller.close()
						} catch (error) {
							controller.enqueue({ type: "error", error })
							controller.close()
						}
					},
				})
				return { stream, response: { headers: Object.fromEntries(response.headers) } }
			},
			async doGenerate(callOptions: LanguageModelV3CallOptions) {
				// Assemble from the stream: one code path, so a fixture that pins
				// streaming pins generation too.
				const { stream } = await this.doStream(callOptions)
				const reader = stream.getReader()
				const content: Array<Record<string, unknown>> = []
				let finish = { unified: "stop", raw: "unknown" } as unknown as ReturnType<Reducer["feed"]>[number]
				let usage: LanguageModelV3Usage = {
					inputTokens: { total: 0, noCache: 0, cacheRead: 0, cacheWrite: 0 },
					outputTokens: { total: 0, text: 0, reasoning: 0 },
				}
				const textOf: Record<string, string> = {}
				const toolsById = new Map<string, { name: string; input: string }>()
				for (;;) {
					const { done, value } = await reader.read()
					if (done) break
					const part = value as Record<string, unknown>
					if (part.type === "text-delta") textOf[String(part.id)] = (textOf[String(part.id)] ?? "") + String(part.delta)
					else if (part.type === "reasoning-delta") {
						content.push({ type: "reasoning", text: String(part.delta) })
					} else if (part.type === "tool-input-start") {
						toolsById.set(String(part.id), { name: String(part.toolName), input: "" })
					} else if (part.type === "tool-input-delta") {
						const buffer = toolsById.get(String(part.id))
						if (buffer) buffer.input += String(part.delta)
					} else if (part.type === "tool-call") {
						content.push({
							type: "tool-call",
							toolCallId: part.toolCallId,
							toolName: part.toolName,
							input: part.input,
						})
					} else if (part.type === "finish") {
						finish = part as never
						usage = part.usage as LanguageModelV3Usage
					}
				}
				for (const [id, value] of Object.entries(textOf)) content.push({ type: "text", text: value, id })
				return {
					content: content as never,
					finishReason: (finish as unknown as { finishReason?: unknown }).finishReason as never,
					usage,
					warnings: [],
				}
			},
		}
	}

	return { languageModel: model }
}
