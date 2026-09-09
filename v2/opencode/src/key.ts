import { readFile } from "node:fs/promises";
import { homedir } from "node:os";

// Key resolution order:
// 1. CMD_API_KEY env override (multi-account/testing, mirrors cmduse)
// 2. opencode auth store (populated by /connect via our auth hook)
// 3. Command Code CLI's auth.json (user logged in via `cmd login`)

export async function resolveKey(
	getAuth?: () => Promise<{ key: string } | undefined>,
): Promise<string> {
	const env = process.env.CMD_API_KEY;
	if (env) return env;
	try {
		const auth = await getAuth?.();
		if (auth?.key) return auth.key;
	} catch {}
	try {
		const text = await readFile(`${homedir()}/.commandcode/auth.json`, "utf8");
		const v = JSON.parse(text);
		if (typeof v.apiKey === "string" && v.apiKey) return v.apiKey;
	} catch {}
	throw new Error(
		"No Command Code API key found. Run /connect, choose Command Code, and paste your key (or `cmd login` in a terminal, or set CMD_API_KEY).",
	);
}
