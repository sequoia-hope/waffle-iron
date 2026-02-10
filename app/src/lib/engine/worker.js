/**
 * Waffle Iron Engine Web Worker (SvelteKit version)
 *
 * Loads the Rust WASM module and processes messages from the main thread.
 * Also loads the libslvs Emscripten WASM module for constraint solving.
 * All engine computation happens in this worker to keep the UI responsive.
 */

import { initSlvs, isSlvsReady, solveSketch } from './slvs-solver.js';

let wasmModule = null;

/**
 * Load the libslvs Emscripten module via fetch+blob to avoid Vite/Rollup
 * trying to resolve the non-bundled Emscripten output.
 */
async function loadSlvsFactory() {
	const resp = await fetch('/pkg/slvs/slvs.js');
	const text = await resp.text();
	// Add ES module exports to the Emscripten output
	const moduleText = text + '\nexport { createSlvsModule };\nexport default createSlvsModule;';
	const blob = new Blob([moduleText], { type: 'text/javascript' });
	const blobUrl = URL.createObjectURL(blob);
	const mod = await import(/* @vite-ignore */ blobUrl);
	URL.revokeObjectURL(blobUrl);
	return mod.default || mod.createSlvsModule;
}

/**
 * Initialize the WASM module.
 * @param {string} wasmUrl - URL to the wasm_bridge.js module
 */
async function initEngine(wasmUrl) {
	try {
		const wasm = await import(/* @vite-ignore */ wasmUrl || '/pkg/wasm_bridge.js');
		await wasm.default();
		wasm.init();
		wasmModule = wasm;

		// Load libslvs constraint solver (non-blocking, graceful failure)
		try {
			const createSlvsModule = await loadSlvsFactory();
			await initSlvs(createSlvsModule);
			console.log('libslvs constraint solver ready');
		} catch (err) {
			console.warn('libslvs solver not available:', err.message);
		}

		self.postMessage({ type: 'ready' });
	} catch (err) {
		self.postMessage({
			type: 'Error',
			message: `WASM initialization failed: ${err.message}`,
			feature_id: null
		});
	}
}

/**
 * Process a UiToEngine message and return the EngineToUi response.
 * @param {object} msg
 * @returns {object}
 */
function processMessage(msg) {
	if (!wasmModule) {
		return {
			type: 'Error',
			message: 'Engine not initialized',
			feature_id: null
		};
	}

	try {
		const jsonInput = JSON.stringify(msg);
		const jsonOutput = wasmModule.process_message(jsonInput);
		return JSON.parse(jsonOutput);
	} catch (err) {
		return {
			type: 'Error',
			message: `Engine error: ${err.message}`,
			feature_id: null
		};
	}
}

/**
 * Handle a sketch solve request using libslvs.
 * @param {object} msg - { type: 'SolveSketchLocal', entities, constraints, positions }
 */
function handleSolveSketch(msg) {
	if (!isSlvsReady()) {
		self.postMessage({
			type: 'SketchSolved',
			positions: msg.positions,
			status: 'solver_not_ready',
			dof: -1,
			failed: []
		});
		return;
	}

	try {
		const t0 = performance.now();
		const result = solveSketch(msg.entities, msg.constraints, msg.positions);
		const elapsed = performance.now() - t0;

		self.postMessage({
			type: 'SketchSolved',
			positions: result.positions,
			status: result.status,
			dof: result.dof,
			failed: result.failed,
			solveTime: elapsed
		});
	} catch (err) {
		self.postMessage({
			type: 'SketchSolved',
			positions: msg.positions,
			status: 'error',
			dof: -1,
			failed: [],
			error: err.message
		});
	}
}

/**
 * Collect mesh data for all features as Transferable typed arrays.
 */
function collectMeshes() {
	if (!wasmModule) return { meshes: [], transferables: [] };

	const meshes = [];
	const transferables = [];

	const features = JSON.parse(wasmModule.get_feature_tree()).features || [];

	for (let i = 0; i < features.length; i++) {
		const vertView = wasmModule.get_mesh_vertices(i);
		const normView = wasmModule.get_mesh_normals(i);
		const idxView = wasmModule.get_mesh_indices(i);

		if (vertView.length === 0) continue;

		const vertices = new Float32Array(vertView);
		const normals = new Float32Array(normView);
		const indices = new Uint32Array(idxView);

		meshes.push({
			featureIndex: i,
			featureId: features[i].id,
			vertices,
			normals,
			indices,
			triangleCount: indices.length / 3
		});

		transferables.push(vertices.buffer, normals.buffer, indices.buffer);
	}

	return { meshes, transferables };
}

self.onmessage = async function (event) {
	const msg = event.data;

	if (msg.type === 'init') {
		await initEngine(msg.wasmUrl);
		return;
	}

	// Intercept sketch solve — handled by libslvs, not Rust engine
	if (msg.type === 'SolveSketchLocal') {
		handleSolveSketch(msg);
		return;
	}

	const response = processMessage(msg);

	if (response.type === 'ModelUpdated') {
		const { meshes, transferables } = collectMeshes();
		response.meshes = meshes;
		self.postMessage(response, transferables);
	} else {
		self.postMessage(response);
	}
};

self.onerror = function (error) {
	self.postMessage({
		type: 'Error',
		message: `Worker error: ${error.message || error}`,
		feature_id: null
	});
};
