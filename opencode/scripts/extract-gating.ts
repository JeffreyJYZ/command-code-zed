// Extracts Command Code's model-category + plan-gating tables from the installed
// CLI bundle (cli.mjs) and writes core/gating.json (consumed by cmduse-core's
// build.rs and the opencode plugin). Run via `bun run extract` after every
// Command Code release that changes the catalog.
//
// ponytail: regex-scrapes a minified bundle — breaks if Command Code renames the
// Fr/Ur/Sr/wr minified vars; upgrade path is pinning a documented endpoint when
// one ships, or re-locating the literals by their stable string anchors below.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

function findCliMjs(): string {
	const roots = [
		join(homedir(), "Library/pnpm"),
		"/usr/lib/node_modules",
		"/usr/local/lib/node_modules",
		join(homedir(), ".npm-global"),
		join(homedir(), ".bun/install/global/node_modules"),
	];
	for (const root of roots) {
		let p = join(root, "command-code/dist/cli.mjs");
		try {
			readFileSync(p);
			return p;
		} catch {}
		// pnpm store layout
		try {
			const links = join(root, "store/v11/links/@/command-code");
			for (const ver of readdirSync(links)) {
				for (const hash of readdirSync(join(links, ver))) {
					p = join(links, ver, hash, "node_modules/command-code/dist/cli.mjs");
					try {
						readFileSync(p);
						return p;
					} catch {}
				}
			}
		} catch {}
	}
	throw new Error("cli.mjs not found — install Command Code CLI (npm i -g command-code)");
}

const src = readFileSync(findCliMjs(), "utf8");

function grab(start: string, end: string): string {
	const i = src.indexOf(start);
	if (i < 0) throw new Error(`anchor not found: ${start}`);
	const j = src.indexOf(end, i);
	if (j < 0) throw new Error(`end anchor not found: ${end}`);
	return src.slice(i + start.length, j);
}

// --- resolve minified identifier values (aliases can shift between releases) ---
const varRe =
	/\b([A-Za-z_$][\w$]*)="(premium|opensource|anthropic|vercel-ai-gateway|openrouter|openai)"/g;
const vars = new Map<string, string>();
for (const m of src.matchAll(varRe)) vars.set(m[1]!, m[2]!);

const lit = (id: string | undefined, fallback: string): string => (id && vars.get(id)) || fallback;

function expand(expr: string): string {
	// $r(X) always = {provider:X, category:"premium"} (single definition in bundle)
	return expr
		.replace(
			/\$r\(([A-Za-z_$][\w$]*)\)/g,
			(_, id) => `{provider:"${lit(id, "anthropic")}",category:"premium"}`,
		)
		.replace(/_r\(\)/g, `{provider:"cai",category:"opensource"}`)
		.replace(/category:([A-Za-z_$][\w$]*)/g, (_, id) => `category:"${lit(id, "opensource")}"`)
		.replace(/provider:([A-Za-z_$][\w$]*)/g, (_, id) => `provider:"${lit(id, "openai")}"`)
		.replace(
			/allowedCategories:\[([^\]]*)\]/g,
			(_, inner: string) =>
				`allowedCategories:[${[...inner.matchAll(/([A-Za-z_$][\w$]*)/g)].map((v) => `"${lit(v[1], "opensource")}"`).join(",")}]`,
		);
}

// --- category table: Fr = { "<model id>": {provider, category} } ---
// getModelCategory looks up Fr DIRECTLY (no canonicalization), so table keys are raw ids
const frRaw = grab("Fr={", "},Ur={");
const categories: Record<string, string> = {};
for (const m of expand(frRaw).matchAll(/"([^"]+)":\{provider:"[^"]*",category:"([^"]+)"\}/g)) {
	categories[m[1]!] = m[2]!;
}

// --- plan table: Ur = { "<planId>": {allowedCategories, blockedModels?} } ---
// plan ids and category values stay as minified var refs (Nr/Dr) until expand below
const urRaw = grab('Ur={"', "},jr=");
const plans: Record<string, { allowedCategories: string[]; blockedModels: string[] }> = {};
for (const m of expand(`{"${urRaw}}`).matchAll(
	/"(individual-[a-z0-9-]+|teams-[a-z0-9-]+)":\{allowedCategories:\[([^\]]*)\](,blockedModels:\[([^\]]*)\])?\}/g,
)) {
	const cats = [...m[2]!.matchAll(/"([a-z]+)"/g)].map((c) => c[1]!);
	const blocked = m[4] ? [...m[4]!.matchAll(/"([^"]+)"/g)].map((b) => b[1]!) : [];
	plans[m[1]!] = { allowedCategories: cats, blockedModels: blocked };
}

// --- known model ids: Sr = new Set([...]) ---
const srRaw = grab('Sr=new Set(["', "])");
const knownFromSet = [...srRaw.matchAll(/"([a-z][^"]*)"/g)]
	.filter((m) => m[1]!.length > 2)
	.map((m) => m[1]!);
// ponytail: KNOWN_MODELS = Set literal ∪ category-table keys; the Set literal
// alone misses models whose spec entries come from spread arrays in the bundle.
// upgrade: alert on extract if regenerated KNOWN_MODELS ⊆ categories (Set-only)
// ever becomes sufficient — the spread-source no longer exists, drop the union.
const known = [...new Set([...knownFromSet, ...Object.keys(categories)])];

// --- deprecated aliases: wr = { old: new } ---
const wrRaw = grab("wr={", "},vr={");
const aliases: Record<string, string> = {};
for (const m of wrRaw.matchAll(/"([^"]+)":"([^"]+)"/g)) aliases[m[1]!] = m[2]!;

// Models the API hard-blocks (403 MODEL_NOT_IN_PLAN) per plan, beyond the
// category rules. NOT in the bundle (the CLI tracks category via serving lane,
// not per-model entitlements) — probed empirically, maintain by hand here.
const hardBlocked: Record<string, string[]> = {
	"individual-goat": [
		"meta/muse-spark-1.1",
		"google/gemini-3.5-flash",
		"google/gemini-3.6-flash",
		"google/gemini-3.5-flash-lite",
		"google/gemini-3.1-flash-lite",
	],
	"individual-go": [
		"meta/muse-spark-1.1",
		"google/gemini-3.5-flash",
		"google/gemini-3.6-flash",
		"google/gemini-3.5-flash-lite",
		"google/gemini-3.1-flash-lite",
		"meta/muse-spark-1.2",
		"meta/muse-spark-1.2-contributor",
		"meta/muse-spark-1.3",
		"meta/muse-spark-1.3-contributor",
	],
};

// Single source for the gating data, consumed by the opencode plugin (import)
// and cmduse-core's build.rs (Rust CLI). src/gating.ts is a thin typed loader.
const out = {
	categories,
	plans,
	knownModels: known,
	aliases,
	hardBlocked,
};
writeFileSync(
	new URL("../../core/gating.json", import.meta.url),
	JSON.stringify(out, null, "\t") + "\n",
);
console.log(
	`wrote core/gating.json: ${Object.keys(categories).length} categories, ${Object.keys(plans).length} plans, ${known.length} known models, ${Object.keys(aliases).length} aliases`,
);
