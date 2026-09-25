// Regenerates src/catalog.ts from the official Command Code CLI package:
//   - models.md (the published model knowledge doc): per-model context,
//     reasoning efforts, $/1M rates (in/out/cache-read), cache-write for
//     Anthropic models, and names
//   - dist/cli.mjs: per-model inputModalities (the listing API carries no
//     capabilities — same source opencode-cmd-provider derives from).
//
//   bun scripts/extract-catalog.ts [--models-md path] [--cli-bundle path]
//
// Never hand-edit the generated file: models, efforts, rates and vision all
// move upstream; this keeps the snapshot honest (like gating.json).
import { writeFile } from "node:fs/promises"

const REGISTRY = "https://registry.npmjs.org/command-code/latest"
/**
 * CDNs in preference order. unpkg 500s on some versions (1.65.2 did while
 * jsdelivr served the same file), so one is not enough to keep the snapshot
 * refreshable unattended.
 */
const CDNS = ["https://unpkg.com", "https://cdn.jsdelivr.net/npm"]
const BUNDLE = (cdn: string, version: string) => `${cdn}/command-code@${version}/dist/cli.mjs`
const MODELS_MD = (cdn: string, version: string) =>
	`${cdn}/command-code@${version}/dist/bundled/command-code-knowledge/reference/models.md`

/** First CDN that serves a non-empty body, with the URL that worked. */
async function fetchFirst(pathFor: (cdn: string, version: string) => string, version: string, label: string): Promise<[string, string]> {
	let last = ""
	for (const cdn of CDNS) {
		const url = pathFor(cdn, version)
		const response = await fetch(url).catch((error) => ({ ok: false, status: 0, text: async () => String(error) }))
		const body = response.ok ? await response.text() : ""
		if (body.length > 0) return [body, url]
		last = `${new URL(url).host} -> ${response.status}`
	}
	throw new Error(`no CDN served ${label} (last: ${last})`)
}
const OUT = new URL("../src/catalog.ts", import.meta.url).pathname

export type InputModality = "text" | "image" | "audio" | "video" | "pdf"

/** $/1M rates as published, plus reasoning efforts when advertised. */
export interface CatalogEntry {
	name: string
	context: number
	efforts: string[] | null
	cost: { input: number; output: number; cacheRead: number; cacheWrite: number }
	modalities: InputModality[]
	/** Cheapest plan that serves the model ("Go" | "GOAT" | "Pro" | "Max"), or
	 * null when the docs do not say. `plans.md` names this column the access
	 * rule, so it is the stable gating source. */
	minPlan: string | null
}

/** `Go and above` → `Go`; `—`/blank → null. */
export function normalizeMinPlan(raw: string | undefined): string | null {
	const value = (raw ?? "").replace(/\s+and above\s*$/i, "").trim()
	if (!value || value === "—" || value === "-") return null
	return value
}

/**
 * Pull `{ id: "...", inputModalities: ["text","image"] }` records out of the
 * minified bundle. `[^{}]*?` keeps the match inside one object literal, so an
 * unrelated `id` earlier in the file cannot pair with a later modalities list.
 * ≤1.65.0 shipped these inline; newer bundles dropped them for a denylist
 * (see `parseTextOnly`), and this returns {} there.
 */
export function parseModalities(bundle: string): Record<string, InputModality[]> {
	const out: Record<string, InputModality[]> = {}
	const re = /id:"([^"]+)"[^{}]*?inputModalities:\[([^\]]*)\]/g
	for (const match of bundle.matchAll(re)) {
		const id = match[1]
		if (!id) continue
		const modalities = [...(match[2] ?? "").matchAll(/"([a-z]+)"/g)]
			.map((m) => m[1] as InputModality)
			.filter(Boolean)
		if (modalities.length) out[id] = modalities
	}
	return out
}

/**
 * Text-only model ids. 1.65+ inverted the modality default: a model accepts
 * images unless it is in this set (`supportsVision` = `inputModalities?.includes
 * ("image") ?? !isKnownTextOnlyModel`), so the list is the only stable anchor
 * left. Canonicalised in the bundle; compared case-insensitively here.
 */
export function parseTextOnly(bundle: string): string[] {
	const match = bundle.match(/Rr=new Set\(\[([^\]]*)\]/)
	if (!match) return []
	return [...(match[1] ?? "").matchAll(/"([^"]+)"/g)].map((m) => m[1] as string).filter(Boolean)
}

/** Explicit record wins; otherwise the CLI's own default (vision unless denied).
 * `textOnly` must hold lowercased ids — `mergeCatalog` builds it that way. */
export function modalitiesFor(
	id: string,
	explicit: Record<string, InputModality[]>,
	textOnly: ReadonlySet<string>,
): InputModality[] {
	const known = explicit[id]
	if (known) return known
	return textOnly.has(id.toLowerCase()) ? ["text"] : ["text", "image"]
}

/** `\`id\` | Name | Context | Efforts | $/1M in/out · cache read | ...` — parsed
 * cell-wise: splitting on pipes first tolerates the optional `(write $…)` tail
 * and any extra prose columns without a brittle whole-line regex. */
const RATE_RE = /^\$([\d.]+)\/\$([\d.]+)\s*·\s*cache\s*\$([\d.]+)(?:\s*\(write\s*\$([\d.]+)\))?$/

function parseCount(raw: string): number {
	const m = raw.trim().match(/^([\d.]+)\s*([MK])?$/i)
	if (!m) return 0
	const value = Number(m[1])
	if (!Number.isFinite(value)) return 0
	const unit = (m[2] ?? "").toUpperCase()
	return Math.round(value * (unit === "M" ? 1_000_000 : unit === "K" ? 1000 : 1))
}

export function parseModelsMd(md: string): Record<string, Omit<CatalogEntry, "modalities">> {
	const out: Record<string, Omit<CatalogEntry, "modalities">> = {}
	for (const line of md.split("\n")) {
		const cells = line.split("|").map((c) => c.trim())
		// ["", "`id`", name, context, efforts, rate, minPlan, bestFor, ""]
		if (cells.length < 8 || !cells[1]?.startsWith("`") || !cells[1]?.endsWith("`")) continue
		const rate = cells[5]?.match(RATE_RE)
		if (!rate) continue
		const [, input, output, cacheRead, cacheWrite] = rate
		const id = cells[1].slice(1, -1)
		const effortsRaw = (cells[4] ?? "").trim()
		out[id] = {
			name: (cells[2] ?? "").trim(),
			context: parseCount(cells[3] ?? ""),
			efforts:
				!effortsRaw || effortsRaw === "—" || effortsRaw === "-"
					? null
					: effortsRaw
							.split(",")
							.map((e) => e.trim())
							.filter(Boolean),
			cost: {
				input: Number(input),
				output: Number(output),
				cacheRead: Number(cacheRead),
				cacheWrite: cacheWrite === undefined ? 0 : Number(cacheWrite),
			},
			minPlan: normalizeMinPlan(cells[6]),
		}
	}
	return out
}

export function mergeCatalog(
	pricing: Record<string, Omit<CatalogEntry, "modalities">>,
	modalities: Record<string, InputModality[]>,
	textOnly: readonly string[] = [],
): Record<string, CatalogEntry> {
	const denied = new Set(textOnly.map((id) => id.toLowerCase()))
	const out: Record<string, CatalogEntry> = {}
	for (const [id, entry] of Object.entries(pricing)) {
		out[id] = { ...entry, modalities: modalitiesFor(id, modalities, denied) }
	}
	return out
}

function render(version: string, models: Record<string, CatalogEntry>): string {
	const entries = Object.entries(models)
		.sort(([a], [b]) => a.localeCompare(b))
		.map(
			([id, e]) =>
				`\t${JSON.stringify(id)}: { name: ${JSON.stringify(e.name)}, context: ${e.context}, ` +
				`efforts: ${e.efforts === null ? "null" : JSON.stringify(e.efforts)}, ` +
				`cost: { input: ${e.cost.input}, output: ${e.cost.output}, cacheRead: ${e.cost.cacheRead}, cacheWrite: ${e.cost.cacheWrite} }, ` +
				`modalities: [${e.modalities.map((m) => `"${m}"`).join(", ")}], ` +
				`minPlan: ${e.minPlan === null ? "null" : JSON.stringify(e.minPlan)} },`,
		)
		.join("\n")
	return `// GENERATED by scripts/extract-catalog.ts — do not edit by hand.
// Source: command-code@${version} (models.md + dist/cli.mjs). Regenerate with
// \`bun scripts/extract-catalog.ts\` after a Command Code model release.
//
// The listing API carries neither capabilities nor rates, so this table is the
// only per-model source. Costs are $/1M as published. Models absent here are
// text-only (OpenCode's default); the wire consumes this table only for models
// returned by the live listing.

export type InputModality = "text" | "image" | "audio" | "video" | "pdf"

/** Per-1M-token rates, plus efforts (null when the model advertises none). */
export interface ModelCost {
	input: number
	output: number
	cacheRead: number
	cacheWrite: number
}

export interface CatalogEntry {
	name: string
	context: number
	efforts: readonly string[] | null
	cost: ModelCost
	modalities: readonly InputModality[]
	/** Cheapest plan that serves the model; plans.md names this the access rule. */
	minPlan: string | null
}

export const CATALOG_VERSION = ${JSON.stringify(version)} as const

/** Every model the CLI table carried, keyed by the API's model id. */
export const MODEL_CATALOG: Readonly<Record<string, CatalogEntry>> = {
${entries}
}

/** Text-only default for any id this table does not list. */
export const TEXT_ONLY: readonly InputModality[] = ["text"]

export function inputModalities(id: string): readonly InputModality[] {
	return MODEL_CATALOG[id]?.modalities ?? TEXT_ONLY
}

export function supportsImage(id: string): boolean {
	return inputModalities(id).includes("image")
}

export function modelCost(id: string): ModelCost | undefined {
	return MODEL_CATALOG[id]?.cost
}

export function minPlan(id: string): string | null {
	return MODEL_CATALOG[id]?.minPlan ?? null
}

/** Reasoning-capable when it advertises explicit effort levels. */
export function isReasoningModel(id: string): boolean {
	const efforts = MODEL_CATALOG[id]?.efforts
	return Array.isArray(efforts) && efforts.length > 0
}

/** opencode effort variants for ctrl+t cycling. */
export function reasoningVariants(id: string): Record<string, { reasoningEffort: string }> | undefined {
	const efforts = MODEL_CATALOG[id]?.efforts
	if (!efforts || efforts.length === 0) return undefined
	return Object.fromEntries(efforts.map((effort) => [effort, { reasoningEffort: effort }]))
}
`
}

async function fetchVersion(): Promise<string> {
	const meta = (await (await fetch(REGISTRY)).json()) as { version?: string }
	if (!meta.version) throw new Error(`no version at ${REGISTRY}`)
	return meta.version
}

if (import.meta.main) {
	const args = process.argv.slice(2)
	const mdArg = args[args.indexOf("--models-md") + 1]
	const bundleArg = args[args.indexOf("--cli-bundle") + 1]
	const version = await fetchVersion()
	const [md, bundle] = await Promise.all([
		mdArg ? await Bun.file(mdArg).text() : (await fetchFirst(MODELS_MD, version, "models.md"))[0],
		bundleArg ? await Bun.file(bundleArg).text() : (await fetchFirst(BUNDLE, version, "cli.mjs"))[0],
	])
	const pricing = parseModelsMd(md)
	const modalities = parseModalities(bundle)
	const textOnly = parseTextOnly(bundle)
	const catalog = mergeCatalog(pricing, modalities, textOnly)
	const ids = Object.keys(catalog)
	if (ids.length === 0) throw new Error("parsed no models — upstream docs shape changed")
	// The docs page can lag a CLI release; modalities-only ids (new models) get
	// the priced-line treatment next refresh. Fail loud, not silently sparse.
	const orphaned = Object.keys(modalities).filter((id) => !(id in pricing))
	if (orphaned.length > 0) {
		console.warn(`warning: ${orphaned.length} modalities-only ids lack a pricing row: ${orphaned.slice(0, 8).join(", ")}`)
	}
	await writeFile(OUT, render(version, catalog))
	console.log(`wrote src/catalog.ts: ${ids.length} models from command-code@${version}`)
}
