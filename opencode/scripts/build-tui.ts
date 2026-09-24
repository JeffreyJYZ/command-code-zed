// Builds dist/tui.js (the "./tui" export) from src/tui.tsx.
//
// A plain `bun build` of TSX is not reactive: JSX props are evaluated when the
// element is created, so `rows={() => …}`-style getters work but `when`/`each`
// and nested reactive reads can freeze at mount and the panel never repaints
// when the session model changes. Compiling with @opentui/solid's transform
// (deferred props + reactive inserts — the same transform opencode's own TUI
// uses) keeps it live.
//
// Both TUI hosts rewrite `@opentui/*` / `solid-js` imports to their own module
// instances at plugin load, so those stay external — and the slice must remain
// a single bundle, because the rewrite only covers the entry's imports.
import { createSolidTransformPlugin } from "@opentui/solid/bun-plugin"

const out = await Bun.build({
	entrypoints: [new URL("../src/tui.tsx", import.meta.url).pathname],
	target: "bun",
	outdir: new URL("../dist", import.meta.url).pathname,
	naming: "tui.js",
	plugins: [createSolidTransformPlugin({ resolvePath: () => null })],
	external: [
		"bun:sqlite", // Bun builtin: must resolve at runtime, not be bundled
		"solid-js",
		"solid-js/*",
		"@opentui/*",
		"@opencode-ai/*",
		"@opencode/plugin",
		"@opencode/plugin/tui",
	],
	minify: false,
})

if (!out.success) {
	for (const log of out.logs) console.error(log)
	process.exit(1)
}
