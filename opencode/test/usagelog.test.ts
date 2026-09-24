import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { logEvent, logUsage, type UsageMessage } from "../src/usagelog";

const VAR = "MPC_USAGE_LOG";

function scratch(): string {
	const path = join(mkdtempSync(join(tmpdir(), "usage-log-")), "usage.jsonl");
	process.env[VAR] = path;
	return path;
}

function lines(path: string): unknown[] {
	return readFileSync(path, "utf8")
		.split("\n")
		.filter(Boolean)
		.map((l) => JSON.parse(l));
}

const assistant: UsageMessage = {
	id: "msg_1",
	role: "assistant",
	providerID: "command-code-anthropic",
	modelID: "claude-sonnet-5",
	cost: 0.0123,
	tokens: { input: 1200, output: 300, cache: { read: 50_000, write: 0 } },
	time: { created: 1_700_000_000_000, completed: 1_700_000_005_000 },
};

describe("usagelog", () => {
	afterEach(() => {
		delete process.env[VAR];
	});

	test("writes one line per finished assistant message", () => {
		const path = scratch();
		logUsage(assistant);
		const rows = lines(path) as Array<Record<string, unknown>>;
		expect(rows).toHaveLength(1);
		const row = rows[0];
		expect(row?.model).toBe("claude-sonnet-5");
		expect(row?.input).toBe(1200);
		expect(row?.cacheRead).toBe(50_000);
		expect(row?.output).toBe(300);
		expect(row?.costUsd).toBeCloseTo(0.0123, 6);
		expect(typeof row?.ts).toBe("string");
	});

	test("ignores user messages, unfinished messages and empty usage", () => {
		const path = scratch();
		logUsage({ ...assistant, role: "user" });
		logUsage({ ...assistant, id: "m2", time: { created: 1 } });
		logUsage({
			...assistant,
			id: "m3",
			tokens: { input: 0, output: 0, cache: { read: 0, write: 0 } },
		});
		expect(() => readFileSync(path, "utf8")).toThrow();
	});

	test("dedupes repeated updates for the same message id", () => {
		const path = scratch();
		const once = { ...assistant, id: "dedupe_1" };
		logUsage(once);
		logUsage(once);
		expect(lines(path)).toHaveLength(1);
	});

	test("logEvent only reacts to message.updated", () => {
		const path = scratch();
		logEvent({ type: "session.idle", properties: {} });
		logEvent({
			type: "message.updated",
			properties: { info: { ...assistant, id: "event_1" } },
		});
		expect(lines(path)).toHaveLength(1);
	});
});
