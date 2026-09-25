// Disk cache for the live model list.
//
// The listing API takes ~2.5s, and a transform that lands mid-startup makes the
// host re-publish provider/model state while the TUI is still painting — which
// read as "the UI waited for the fetch" even though the snapshot was registered
// in 1ms. Caching the last good list lets a warm start merge the *fresh* list
// during registration, so the background refresh usually changes nothing and
// fires no second update at all.
import { mkdir, readFile, writeFile } from "node:fs/promises"
import { homedir } from "node:os"
import { join } from "node:path"

export interface CachedModel {
	id: string
	name: string
	contextLength: number
}

export interface ModelsCache {
	fetchedAt: number
	claude: CachedModel[]
	open: CachedModel[]
}

function cachePath(): string {
	const base = process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache")
	return join(base, "command-code", "models.json")
}

/** Never throws: a missing or corrupt cache is simply "no cache". */
export async function readModelsCache(): Promise<ModelsCache | undefined> {
	try {
		const parsed = JSON.parse(await readFile(cachePath(), "utf8")) as ModelsCache
		if (!Array.isArray(parsed.claude) || !Array.isArray(parsed.open)) return undefined
		return parsed
	} catch {
		return undefined
	}
}

/** Best-effort write; diagnostics must never break setup. */
export async function writeModelsCache(cache: ModelsCache): Promise<void> {
	try {
		const path = cachePath()
		await mkdir(join(path, ".."), { recursive: true })
		await writeFile(path, JSON.stringify(cache))
	} catch {
		// cache is best-effort
	}
}

/** True when two lists differ by ids (the only thing a picker shows). */
export function idsDiffer(a: readonly CachedModel[], b: readonly CachedModel[]): boolean {
	if (a.length !== b.length) return true
	const left = new Set(a.map((m) => m.id))
	return b.some((m) => !left.has(m.id))
}

/** Path for tests and diagnostics. */
export function modelsCachePath(): string {
	return cachePath()
}
