export const API_BASE = "https://api.commandcode.ai";

export function authHeaders(key: string): Record<string, string> {
	return { Authorization: `Bearer ${key}`, Accept: "application/json" };
}

export async function getJson(path: string, key: string, timeoutMs = 10_000): Promise<unknown> {
	const resp = await fetch(`${API_BASE}${path}`, {
		headers: authHeaders(key),
		signal: AbortSignal.timeout(timeoutMs),
	});
	const body = await resp.json().catch(() => null);
	if (!resp.ok) {
		const msg =
			(body !== null &&
			typeof body === "object" &&
			"message" in body &&
			typeof body.message === "string"
				? body.message
				: undefined) ?? resp.statusText;
		throw new Error(`${path}: HTTP ${resp.status} — ${msg}`);
	}
	return body;
}

// --- endpoint response shapes (only what we read) ---

export type Whoami = {
	success: boolean;
	user?: { name?: string; userName?: string };
};

export type SubData = {
	status: string;
	planId: string;
	currentPeriodStart?: string;
	currentPeriodEnd?: string;
};

export type Window = {
	used: number;
	cap: number;
	exceeded?: boolean;
	resetAt?: number;
};

export type Credits = {
	credits: {
		monthlyCredits: number;
		purchasedCredits?: number;
		freeCredits?: number;
	};
	windowLimits?: { fiveHour?: Window; weekly?: Window };
};

export type Summary = {
	totalCount?: number;
	totalCost?: number;
	successRate?: number;
	totalTokensIn?: number;
	totalTokensOut?: number;
};

export type ProviderModelsResp = {
	data: Array<{
		id: string;
		name?: string;
		context_length?: number;
		owned_by?: string;
	}>;
};

export async function whoami(key: string): Promise<Whoami> {
	return getJson("/alpha/whoami", key) as Promise<Whoami>;
}

export async function subscriptions(key: string): Promise<SubData> {
	const r = (await getJson("/alpha/billing/subscriptions", key)) as {
		data?: Partial<SubData>;
	};
	return {
		status: r.data?.status ?? "none",
		planId: r.data?.planId ?? "free",
		currentPeriodStart: r.data?.currentPeriodStart,
		currentPeriodEnd: r.data?.currentPeriodEnd,
	};
}

export async function credits(key: string): Promise<Credits> {
	const r = (await getJson("/alpha/billing/credits", key)) as Partial<Credits>;
	return {
		credits: {
			monthlyCredits: r.credits?.monthlyCredits ?? 0,
			purchasedCredits: r.credits?.purchasedCredits ?? 0,
			freeCredits: r.credits?.freeCredits ?? 0,
		},
		windowLimits: r.windowLimits,
	};
}

export async function usageSummary(key: string): Promise<Summary> {
	return getJson("/alpha/usage/summary", key) as Promise<Summary>;
}

export async function providerModels(key: string): Promise<ProviderModelsResp> {
	return getJson("/provider/v1/models", key, 15_000) as Promise<ProviderModelsResp>;
}
