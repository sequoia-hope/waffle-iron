/**
 * Waffle Iron Engine Web Worker (SvelteKit version)
 *
 * Loads the Rust WASM module and processes messages from the main thread.
 * All engine computation happens in this worker to keep the UI responsive.
 */

let wasmModule = null;

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
