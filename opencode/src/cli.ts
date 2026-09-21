// Spawn the cmduse CLI (Rust, cmduse-core) for ALL usage rendering: plan,
// credits, 5-hour/weekly windows, pace — core logic lives in the Rust core,
// this plugin only wraps its output. Piped stdout is plain text (the CLI
// disables SGR when stdout is not a tty), so the output is safe to return
// verbatim as tool content or post as a synthetic message.

import { spawn } from "node:child_process";

/** Fallback lookup paths: the opencode background service's PATH often lacks
 * the homebrew prefix, so the bare name alone is not enough. */
const CANDIDATES = ["cmduse", "/opt/homebrew/bin/cmduse", "/usr/local/bin/cmduse"];

/** Default invocation: one-shot dashboard, no colors / no live redraw. */
const DEFAULT_ARGS = ["-1", "--plain"];

/** cmduse subcommands — when the arg starts with one, pass it through
 * untouched; otherwise it is extra flags for the one-shot dashboard. */
const SUBCOMMANDS = new Set([
	"plans",
	"models",
	"daily",
	"hourly",
	"model",
	"session",
	"statusline",
	"config",
]);

/** Shell-words split: whitespace-separated, with single/double quoting and
 * backslash escapes ("--tz +05:30", arg='a b'). Unterminated quotes close at
 * end of input. */
export function splitCliArgs(input: string): string[] {
	const out: string[] = [];
	let cur = "";
	let has = false;
	let quote: '"' | "'" | undefined;
	for (let i = 0; i < input.length; i++) {
		const ch = input[i] ?? "";
		if (quote) {
			if (ch === quote) {
				quote = undefined;
			} else if (ch === "\\" && quote === '"' && i + 1 < input.length) {
				cur += input[++i];
			} else {
				cur += ch;
			}
			continue;
		}
		if (ch === '"' || ch === "'") {
			quote = ch;
			has = true;
		} else if (ch === "\\") {
			if (i + 1 < input.length) cur += input[++i];
			has = true;
		} else if (/\s/.test(ch)) {
			if (has) out.push(cur);
			cur = "";
			has = false;
		} else {
			cur += ch;
			has = true;
		}
	}
	if (has) out.push(cur);
	return out;
}

/** argv for a cmduse invocation: the default one-shot dashboard plus any
 * pass-through flags, or a subcommand invocation ("plans",
 * "--tz +05:30 daily") passed through untouched. */
export function buildCmduseArgs(arg: string): string[] {
	const tokens = splitCliArgs(arg);
	if (!tokens.length) return DEFAULT_ARGS;
	// Subcommands may follow their own flags (--tz +05:30 daily), so scan all
	// tokens; a dashboard invocation never names one.
	if (tokens.some((t) => SUBCOMMANDS.has(t))) return tokens;
	return [...DEFAULT_ARGS, ...tokens];
}

export type RunOptions = {
	signal?: AbortSignal;
	/** Extra env for the child (CMD_API_KEY when the plugin resolved one). */
	env?: Record<string, string>;
	/** Overridable for tests. */
	candidates?: readonly string[];
};

function stderrTail(text: string): string {
	const lines = text.trimEnd().split("\n");
	return lines.slice(-3).join("\n").slice(-400);
}

function runOnce(bin: string, args: string[], opts: RunOptions): Promise<string> {
	return new Promise((resolve, reject) => {
		const child = spawn(bin, args, {
			env: { ...process.env, ...(opts.env ?? {}) },
			stdio: ["ignore", "pipe", "pipe"],
		});
		const out: Buffer[] = [];
		const err: Buffer[] = [];
		const onAbort = () => child.kill("SIGTERM");
		opts.signal?.addEventListener("abort", onAbort, { once: true });
		child.stdout.on("data", (d: Buffer) => out.push(d));
		child.stderr.on("data", (d: Buffer) => err.push(d));
		child.on("error", (e) => {
			opts.signal?.removeEventListener("abort", onAbort);
			reject(e);
		});
		child.on("close", (code, signal) => {
			opts.signal?.removeEventListener("abort", onAbort);
			const stdout = Buffer.concat(out).toString("utf8");
			if (opts.signal?.aborted) {
				reject(new Error("cmduse invocation aborted"));
			} else if (code === 0) {
				resolve(stdout);
			} else {
				const why = signal ? `killed by ${signal}` : `exit ${code}`;
				const tail = stderrTail(Buffer.concat(err).toString("utf8"));
				reject(new Error(`cmduse ${args.join(" ")}: ${why}${tail ? `\n${tail}` : ""}`));
			}
		});
	});
}

/** Run cmduse and return its plain-text stdout. Tries PATH, then common
 * homebrew locations; every candidate failing with ENOENT yields a single
 * install-hint error. */
export async function runCmduse(arg: string, opts: RunOptions = {}): Promise<string> {
	const args = buildCmduseArgs(arg);
	const candidates = opts.candidates ?? CANDIDATES;
	let last: unknown;
	for (const bin of candidates) {
		try {
			return await runOnce(bin, args, opts);
		} catch (e) {
			last = e;
			if ((e as NodeJS.ErrnoException)?.code !== "ENOENT") throw e;
		}
	}
	throw new Error(
		`cmduse binary not found (install: brew install JeffreyJYZ/tap/cmduse)${last instanceof Error && last.message !== last.name ? ` — ${last.message}` : ""}`,
		{ cause: last },
	);
}
