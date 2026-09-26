import { describe, expect, test } from "bun:test"
import { factsFrom, sha1Hex, verdict } from "../scripts/verify-release"

const packument = {
	"dist-tags": { latest: "0.3.3" },
	versions: { "0.3.3": { dist: { tarball: "https://example.test/p.tgz" } } },
}

describe("verify-release helpers", () => {
	test("reads latest, presence and the tarball url", () => {
		expect(factsFrom(packument, "0.3.3")).toEqual({
			latest: "0.3.3",
			versionPresent: true,
			tarballUrl: "https://example.test/p.tgz",
		})
	})
	test("falls back to the conventional scoped tarball path", () => {
		expect(factsFrom({}, "0.3.3").tarballUrl).toBe(
			"https://registry.npmjs.org/@jeffreyjyz%2Fopencode-command-code/-/opencode-command-code-0.3.3.tgz",
		)
	})
	test("flags a lagging latest even when the version exists", () => {
		const line = verdict("0.3.3", { latest: "0.3.2", versionPresent: true, tarballUrl: "u" }, 200, "a", "a")
		expect(line).toContain("latest not yet moved")
		expect(line).toContain("MATCH")
	})
	test("flags a shasum mismatch", () => {
		expect(verdict("0.3.3", factsFrom(packument, "0.3.3"), 200, "a", "b")).toContain("MISMATCH")
	})
	test("sha1 of a known buffer", () => {
		expect(sha1Hex(new TextEncoder().encode("abc"))).toBe("a9993e364706816aba3e25717850c26c9cd0d89d")
	})
})
