/**
 * Reactive sketch tool state using Svelte 5 runes.
 *
 * tools.js is a plain .js file, so its variables can't be $state.
 * This .svelte.js module holds the reactive state that SketchRenderer
 * needs to track: preview geometry, snap indicator, and snap candidates.
 */

/** @type {{ type: string, data: any } | null} */
let currentPreview = $state(null);

/** @type {import('./snap.js').SnapIndicator | null} */
let currentSnapIndicator = $state(null);

/** @type {Array<{ type: string, x: number, y: number, entityId?: number }>} */
let currentSnapCandidates = $state([]);

// -- Getters --

/** @returns {{ type: string, data: any } | null} */
export function getPreview() {
	return currentPreview;
}

/** @returns {import('./snap.js').SnapIndicator | null} */
export function getSnapIndicator() {
	return currentSnapIndicator;
}

/** @returns {Array<{ type: string, x: number, y: number, entityId?: number }>} */
export function getSnapCandidates() {
	return currentSnapCandidates;
}

// -- Setters (called from tools.js) --

/** @param {{ type: string, data: any } | null} value */
export function setPreview(value) {
	currentPreview = value;
}

/** @param {import('./snap.js').SnapIndicator | null} value */
export function setSnapIndicator(value) {
	currentSnapIndicator = value;
}

/** @param {Array<{ type: string, x: number, y: number, entityId?: number }>} value */
export function setSnapCandidates(value) {
	currentSnapCandidates = value;
}
