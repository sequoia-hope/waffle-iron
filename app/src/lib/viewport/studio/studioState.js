/**
 * Live diagnostic state of the Studio PBR Pipeline (`window.__waffle.getStudioStats()`).
 * Kept in a plain JS module so `store.svelte.js`, `StudioStageSurface.svelte`,
 * and `StudioPipeline.svelte` share it without circular component imports.
 */

const stats = {
	active: false,
	environmentId: 'studio-softbox',
	environmentLoaded: false,
	environmentLoading: false,
	surfaceId: 'none',
	surfaceLoaded: true,
	surfaceLoading: false,
	backdropVisible: false,
	backdropSharpLoaded: false,
	backdropSharpLoading: false,
	sampleCount: 0,
	targetSamples: 8,
	settled: false,
	passCount: 0,
	aoEnabled: false,
	ssrEnabled: false,
	groundShadowVisible: false,
	floatRenderable: false
};

let surfaceRevision = 0;

export function getStudioSurfaceRevision() {
	return surfaceRevision;
}

export function bumpStudioSurfaceRevision() {
	surfaceRevision += 1;
}

export function isStudioSurfaceLoading() {
	return Boolean(stats.surfaceLoading);
}

/** @type {(() => boolean) | null} */
let isCurrentCallback = null;

/**
 * Register a synchronous freshness predicate from `StudioPipeline.svelte`.
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
	if (!isFresh || stats.environmentLoading || stats.surfaceLoading || stats.backdropSharpLoading) {
		return {
			...stats,
			sampleCount: 0,
			settled: false
		};
	}
	return { ...stats };
}
