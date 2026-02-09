/**
 * Engine state store using Svelte 5 runes.
 *
 * Manages reactive state for the WASM engine, including
 * feature tree, mesh data, and engine status.
 */

import { EngineBridge } from './bridge.js';

/** @type {{ features: Array<any>, active_index: number | null }} */
let featureTree = $state({ features: [], active_index: null });

/** @type {Array<{ featureId: string, vertices: Float32Array, normals: Float32Array, indices: Uint32Array, triangleCount: number }>} */
let meshes = $state([]);

let engineReady = $state(false);

/** @type {string | null} */
let lastError = $state(null);

let rebuildTime = $state(0);

let statusMessage = $state('Initializing...');

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
