import { describe, expect, test } from "bun:test";
import tuiPlugin from "../src/tui";

// Guards the package ./tui entrypoint: a broken/missing default export here
// means the CLI plugin silently fails to load and /usage never appears.
describe("tui plugin definition", () => {
	test("exports a V2 definition with id and setup", () => {
		expect(tuiPlugin.id).toBe("command-code.tui");
		expect(typeof tuiPlugin.setup).toBe("function");
	});
});
