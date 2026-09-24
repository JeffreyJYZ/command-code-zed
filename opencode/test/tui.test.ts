import { describe, expect, test } from "bun:test";
import tuiPlugin from "../src/tui";

type Claim = { append?: string; render: (input: never) => unknown };

/** Minimal v2 host: captures the slot claims setup registers. */
function captureSetup(): Claim[] {
	const claims: Claim[] = [];
	const ctx = {
		ui: { slot: (claim: Claim) => void claims.push(claim) },
		keymap: { layer: () => {} },
	};
	tuiPlugin.setup?.(ctx as never);
	return claims;
}

// Guards the package ./tui entrypoint: a broken/missing default export here
// means the CLI plugin silently fails to load and /usage never appears.
describe("tui plugin definition", () => {
	test("exports a V2 definition with id and setup", () => {
		expect(tuiPlugin.id).toBe("command-code.tui");
		expect(typeof tuiPlugin.setup).toBe("function");
	});

	test("registers both the sidebar and the keymap mount", () => {
		const claims = captureSetup();
		expect(claims.map((claim) => claim.append).sort()).toEqual([
			"app",
			"sidebar.content",
		]);
	});

	test("app-slot mount returns void, not null (no stray prompt line)", () => {
		// null leaves an empty box that paints a line in the prompt area;
		// opencode's own /btw claim returns void instead.
		const app = captureSetup().find((claim) => claim.append === "app");
		expect(app).toBeDefined();
		expect(app?.render(undefined as never)).toBeUndefined();
	});
});
