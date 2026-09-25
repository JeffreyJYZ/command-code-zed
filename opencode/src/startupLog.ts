// Startup timing, written where it can actually be read.
//
// A plugin's `console.log` runs in opencode's server process and is NOT captured
// by its file logger, so the one line that explains a slow start was invisible.
// This appends it to our own cache file instead (best-effort: a failure here
// must never affect plugin setup).
import { mkdir, appendFile } from "node:fs/promises"
import { homedir } from "node:os"
import { join } from "node:path"

export interface SetupTimings {
	/** Resolving the local key (env + ~/.commandcode/auth.json). */
	key: number
	/** Provider + snapshot + aisdk hooks registered (this gates /model). */
	register: number
	/** Integration connection lookup. */
	connection: number
	/** Live model list fetch + merge. */
	refresh: number
	/** Models returned by the live list, across lanes. */
	models: number
}

/** The line format, pinned by a test so tooling can parse it. */
export function setupTimingLine(t: SetupTimings): string {
	return `setup: key=${t.key}ms register=${t.register}ms connection=${t.connection}ms refresh=${t.refresh}ms models=${t.models}`
}

/** `$XDG_CACHE_HOME/command-code/startup.log`, alongside the sidebar's catalog cache. */
export function startupLogPath(): string {
	const base = process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache")
	return join(base, "command-code", "startup.log")
}

/** Append one line; never throws. */
export async function writeStartupLine(line: string): Promise<void> {
	try {
		const path = startupLogPath()
		await mkdir(join(path, ".."), { recursive: true })
		await appendFile(path, `[${new Date().toISOString()}] ${line}\n`)
	} catch {
		// diagnostics only
	}
}
