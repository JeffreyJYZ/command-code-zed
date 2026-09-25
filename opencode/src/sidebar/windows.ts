// Rolling-window lengths and elapsed fraction, mirroring cmduse-core
// (`core/src/lib.rs`): the API serves only `resetAt` + `used`/`cap`, so the
// window length is implied by the window's name. The sidebar draws the elapsed
// fraction itself (cmduse's own render owns the dashboard), which means the TS
// port re-implements `elapsed_pct` — kept honest by the shared conformance
// vectors asserted in test/conformance.test.ts.

/** 5-hour window length in seconds. Mirrors cmduse_core::FIVE_HOUR_SECS. */
export const FIVE_HOUR_SECS = 5 * 3600
/** Weekly window length in seconds. Mirrors cmduse_core::WEEKLY_SECS. */
export const WEEKLY_SECS = 7 * 86400

/**
 * Percentage of a rolling window that has elapsed, or undefined when the
 * window has not started (or has no reset time). `nowSecs` is whole seconds —
 * the same unit cmduse-core's vectors pin.
 */
export function elapsedPct(
	resetAtMs: number | undefined,
	durSecs: number,
	nowSecs: number,
): number | undefined {
	if (typeof resetAtMs !== "number" || !Number.isFinite(resetAtMs)) return undefined
	const resetSecs = Math.floor(resetAtMs / 1000)
	// cmduse-core uses u64 `checked_sub`, so a reset before the window length
	// (underflow) is "no window", not a negative start.
	if (resetSecs < durSecs) return undefined
	const start = resetSecs - durSecs
	if (nowSecs < start) return undefined // window hasn't started
	const pct = ((nowSecs - start) / durSecs) * 100
	return Math.round(Math.min(100, Math.max(0, pct)))
}
