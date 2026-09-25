import { describe, expect, test } from "bun:test"
import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { idsDiffer, readModelsCache, writeModelsCache } from "../src/modelsCache"

const m = (id: string) => ({ id, name: id, contextLength: 0 })

describe("models cache", () => {
	test("round-trips a list and reports no difference", async () => {
		process.env.XDG_CACHE_HOME = mkdtempSync(join(tmpdir(), "cc-models-"))
		await writeModelsCache({ fetchedAt: 1, claude: [m("claude-sonnet-5")], open: [m("deepseek/x")] })
		const read = await readModelsCache()
		expect(read?.claude.map((x) => x.id)).toEqual(["claude-sonnet-5"])
		expect(idsDiffer(read!.open, [m("deepseek/x")])).toBe(false)
		delete process.env.XDG_CACHE_HOME
	})

	test("missing cache is undefined, not a throw", async () => {
		process.env.XDG_CACHE_HOME = join(tmpdir(), "cc-models-absent")
		expect(await readModelsCache()).toBeUndefined()
		delete process.env.XDG_CACHE_HOME
	})

	test("detects added and removed ids", () => {
		expect(idsDiffer([m("a")], [m("a"), m("b")])).toBe(true)
		expect(idsDiffer([m("a"), m("b")], [m("a")])).toBe(true)
		expect(idsDiffer([m("a"), m("b")], [m("b"), m("a")])).toBe(false)
	})
})
