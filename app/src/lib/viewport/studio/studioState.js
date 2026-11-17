/**
 * Live diagnostic state of the Studio PBR Pipeline (`window.__waffle.getStudioStats()`).
 * Kept in a plain JS module so `store.svelte.js` and `StudioPipeline.svelte`
 * share it without circular component imports.
 */

const stats = {
	active: false,
	environmentId: 'studio-softbox',
	environmentLoaded: false,
	environmentLoading: false,
	sampleCount: 0,
	targetSamples: 8,
	settled: false,
	passCount: 0,
	aoEnabled: false,
	ssrEnabled: false,
	groundShadowVisible: false,
	backdropVisible: false,
	floatRenderable: false
};

/** @type {(() => boolean) | null} */
let isCurrentCallback = null;

/**
 * Register a synchronous freshness predicate from `StudioPipeline.svelte` so
 * callers polling `getStudioStats().settled` immediately after mutating settings,
 * materials, or camera state never observe a stale `settled: true` from a
 * previous frame before the next animation frame runs.
 * @param {(() => boolean) | null} fn
 */
export function setStudioFreshnessCheck(fn) {
	isCurrentCallback = fn;
}

/** @param {Partial<typeof stats>} patch */
export function updateStudioStats(patch) {
	Object.assign(stats, patch);
}

/** @returns {Readonly<typeof stats>} */
export function readStudioStats() {
	if (!stats.active) {
		return { ...stats, settled: false, sampleCount: 0 };
	}
	const isFresh = isCurrentCallback ? isCurrentCallback() : true;
	if (!isFresh || stats.environmentLoading) {
		return {
			...stats,
			sampleCount: 0,
			settled: false
		};
	}
	return { ...stats };
}
