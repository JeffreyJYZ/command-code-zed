import { describe, expect, test } from "bun:test";
import { buildCmduseArgs, runCmduse, splitCliArgs } from "../src/cli";

describe("splitCliArgs", () => {
	test("whitespace split", () => {
		expect(splitCliArgs("--tz +05:30 daily")).toEqual(["--tz", "+05:30", "daily"]);
	});
	test("multiple spaces / tabs collapse", () => {
		expect(splitCliArgs("  a\t b   c ")).toEqual(["a", "b", "c"]);
	});
	test("double quotes keep spaces", () => {
		expect(splitCliArgs('--model "gpt 5.5"')).toEqual(["--model", "gpt 5.5"]);
	});
	test("single quotes keep spaces literally", () => {
		expect(splitCliArgs("'a b' c")).toEqual(["a b", "c"]);
	});
	test("backslash escapes next char outside quotes", () => {
		expect(splitCliArgs("a\\ b")).toEqual(["a b"]);
	});
	test("empty / no tokens", () => {
		expect(splitCliArgs("")).toEqual([]);
		expect(splitCliArgs("   ")).toEqual([]);
	});
});

describe("buildCmduseArgs", () => {
	test("default dashboard", () => {
		expect(buildCmduseArgs("")).toEqual(["-1", "--plain"]);
	});
	test("subcommand passed through untouched", () => {
		expect(buildCmduseArgs("plans")).toEqual(["plans"]);
		expect(buildCmduseArgs("daily --days 14")).toEqual(["daily", "--days", "14"]);
	});
	test("bare flags prepend the one-shot dashboard", () => {
		expect(buildCmduseArgs("--local")).toEqual(["-1", "--plain", "--local"]);
		expect(buildCmduseArgs("--tz +05:30 hourly")).toEqual(["--tz", "+05:30", "hourly"]);
	});
});

describe("runCmduse", () => {
	test("real binary on PATH (smoke)", async () => {
		const out = await runCmduse("--version");
		expect(out.trim()).toMatch(/\d+\.\d+\.\d+/);
	});
	test("ENOENT across all candidates yields install hint", async () => {
		try {
			await runCmduse("", {
				candidates: ["definitely-not-cmduse-xyz", "/no/such/cmduse"],
			});
			throw new Error("expected runCmduse to reject");
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			expect(msg).toContain("cmduse binary not found");
			expect(msg).toContain("brew install JeffreyJYZ/tap/cmduse");
		}
	});
	test("non-ENOENT errors surface immediately", async () => {
		try {
			// /dev/null exists but is not executable → EACCES, not ENOENT.
			await runCmduse("--version", { candidates: ["/dev/null"] });
			throw new Error("expected runCmduse to reject");
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			expect(msg).not.toContain("cmduse binary not found");
		}
	});
});
