/**
 * Engine state store using Svelte 5 runes.
 *
 * Manages reactive state for the WASM engine, including
 * feature tree, mesh data, and engine status.
 */

import { EngineBridge } from './bridge.js';

/** @type {{ features: Array<any>, active_index: number | null }} */
let featureTree = $state({ features: [], active_index: null });

/** @type {Array<{ featureId: string, vertices: Float32Array, normals: Float32Array, indices: Uint32Array, triangleCount: number, faceRanges?: Array<{geom_ref: any, start_index: number, end_index: number}> }>} */
let meshes = $state([]);

let engineReady = $state(false);

/** @type {string | null} */
let lastError = $state(null);

let rebuildTime = $state(0);

let statusMessage = $state('Initializing...');

/** @type {any | null} */
let hoveredRef = $state(null);

/** @type {Array<any>} */
let selectedRefs = $state([]);

/** @type {EngineBridge | null} */
let bridge = null;

/**
 * Initialize the engine bridge and WASM worker.
 */
export async function initEngine() {
	if (bridge) return;

	bridge = new EngineBridge();

	bridge.on('modelUpdated', (msg) => {
		if (msg.feature_tree) {
			featureTree = msg.feature_tree;
		}
		if (msg.meshes) {
			meshes = msg.meshes;
		}
		lastError = null;
		statusMessage = `Model updated (${meshes.length} ${meshes.length === 1 ? 'body' : 'bodies'})`;
	});

	bridge.on('error', (msg) => {
		lastError = msg.message;
		statusMessage = `Error: ${msg.message}`;
	});

	try {
		statusMessage = 'Loading WASM engine...';
		await bridge.init('/pkg/wasm_bridge.js');
		engineReady = true;
		lastError = null;
		statusMessage = 'Engine ready';
		console.log('Engine ready');
	} catch (err) {
		lastError = /** @type {Error} */ (err).message;
		statusMessage = `Failed to load engine: ${lastError}`;
		console.error('Engine initialization failed:', err);
	}
}

/**
 * Send a command to the engine.
 * @param {object} message - UiToEngine message
 * @returns {Promise<object>} EngineToUi response
 */
export async function send(message) {
	if (!bridge) {
		throw new Error('Engine not initialized');
	}
	return bridge.send(message);
}

/**
 * Get reactive engine state.
 */
export function getFeatureTree() {
	return featureTree;
}

export function getMeshes() {
	return meshes;
}

export function isEngineReady() {
	return engineReady;
}

export function getLastError() {
	return lastError;
}

export function getRebuildTime() {
	return rebuildTime;
}

export function getStatusMessage() {
	return statusMessage;
}

export function getHoveredRef() {
	return hoveredRef;
}

export function getSelectedRefs() {
	return selectedRefs;
}

/**
 * Set the hovered geometry reference.
 * @param {any | null} ref
 */
export function setHoveredRef(ref) {
	hoveredRef = ref;
	if (bridge && engineReady) {
		bridge.send({ type: 'HoverEntity', geom_ref: ref });
	}
}

/**
 * Select a geometry reference. Supports multi-select with additive flag.
 * @param {any | null} ref
 * @param {boolean} additive - If true, toggle selection; if false, replace selection
 */
export function selectRef(ref, additive = false) {
	if (!ref) {
		selectedRefs = [];
		return;
	}

	if (additive) {
		const idx = selectedRefs.findIndex((r) => geomRefEquals(r, ref));
		if (idx >= 0) {
			selectedRefs = [...selectedRefs.slice(0, idx), ...selectedRefs.slice(idx + 1)];
		} else {
			selectedRefs = [...selectedRefs, ref];
		}
	} else {
		selectedRefs = [ref];
	}

	if (bridge && engineReady) {
		for (const r of selectedRefs) {
			bridge.send({ type: 'SelectEntity', geom_ref: r });
		}
	}
}

/**
 * Clear all selections.
 */
export function clearSelection() {
	selectedRefs = [];
}

/**
 * Check if two GeomRefs refer to the same entity.
 * @param {any} a
 * @param {any} b
 * @returns {boolean}
 */
export function geomRefEquals(a, b) {
	if (!a || !b) return false;
	return (
		a.kind?.type === b.kind?.type &&
		a.anchor?.type === b.anchor?.type &&
		a.anchor?.feature_id === b.anchor?.feature_id &&
		a.selector?.type === b.selector?.type &&
		JSON.stringify(a.selector) === JSON.stringify(b.selector)
	);
}

/**
 * Check if a GeomRef is currently selected.
 * @param {any} ref
 * @returns {boolean}
 */
export function isSelected(ref) {
	return selectedRefs.some((r) => geomRefEquals(r, ref));
}
