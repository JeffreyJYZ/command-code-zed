// Per-request usage log for mpc (`mpc --usage`).
//
// opencode sees every request this provider serves, including usage the
// CommandCode account API never breaks down per model. Appending one line per
// finished assistant message gives a complete, local, per-model mix.
import { appendFileSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

export interface UsageMessage {
	id?: string;
	role?: string;
	providerID?: string;
	modelID?: string;
	cost?: number;
	tokens?: {
		input?: number;
		output?: number;
		cache?: { read?: number; write?: number };
	};
	time?: { created?: number; completed?: number };
}

/** `$MPC_USAGE_LOG`, else `$XDG_CACHE_HOME/mpc/usage.jsonl`, else `~/.cache/…`. */
export function usageLogPath(): string {
	if (process.env.MPC_USAGE_LOG) return process.env.MPC_USAGE_LOG;
	const base = process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache");
	return join(base, "mpc", "usage.jsonl");
}

// message.updated fires repeatedly while streaming; only the final update per
// message has `time.completed`, and we remember ids so a re-emit can't double
// count. Bounded so a long session can't grow it forever.
const logged = new Set<string>();
const MAX_SEEN = 5_000;

function numberOr(value: unknown): number {
	return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

/** Append one JSONL line for a finished assistant message. Never throws. */
export function logUsage(info: UsageMessage | undefined): void {
	try {
		if (!info || info.role !== "assistant" || !info.tokens) return;
		const completed = info.time?.completed;
		if (!completed) return;
		if (info.id) {
			if (logged.has(info.id)) return;
			if (logged.size >= MAX_SEEN) logged.clear();
			logged.add(info.id);
		}
		const input = numberOr(info.tokens.input);
		const output = numberOr(info.tokens.output);
		const cacheRead = numberOr(info.tokens.cache?.read);
		const cacheWrite = numberOr(info.tokens.cache?.write);
		if (input + output + cacheRead + cacheWrite === 0) return;

		const line = JSON.stringify({
			ts: new Date(completed).toISOString(),
			provider: info.providerID,
			model: info.modelID,
			input,
			cacheRead,
			cacheWrite,
			output,
			costUsd: numberOr(info.cost),
			messageID: info.id,
		});
		const path = usageLogPath();
		mkdirSync(dirname(path), { recursive: true });
		appendFileSync(path, `${line}\n`);
	} catch {
		// logging must never break a request
	}
}

/** Narrow an opencode event down to an assistant message update. */
export function logEvent(event: unknown): void {
	const e = event as {
		type?: string;
		properties?: { info?: UsageMessage };
	};
	if (e?.type !== "message.updated") return;
	logUsage(e.properties?.info);
}
