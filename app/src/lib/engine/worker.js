/**
 * Waffle Iron Engine Web Worker (SvelteKit version)
 *
 * Loads the Rust WASM module and processes messages from the main thread.
 * All engine computation happens in this worker to keep the UI responsive.
 */

let wasmModule = null;
let basePath = '';

/**
 * Initialize the WASM module.
 * @param {string} wasmUrl - URL to the wasm_bridge.js module
 */
async function initEngine(wasmUrl) {
	try {
		lastWasmUrl = wasmUrl;
		const wasm = await import(/* @vite-ignore */ wasmUrl);
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

/** URL used for WASM module init — stored for crash recovery re-init. */
let lastWasmUrl = '';

/**
 * Process a UiToEngine message and return the EngineToUi response.
 *
 * If the WASM module traps (e.g., `unreachable` from an uncaught panic),
 * the module is marked dead and a `needsRestart` flag is set so the bridge
 * can auto-reinitialize the worker.
 *
 * @param {object} msg
 * @returns {object}
 */
function processMessage(msg) {
	if (!wasmModule) {
		return {
			type: 'Error',
			message: 'Engine not initialized',
			feature_id: null,
			needsRestart: true
		};
	}

	try {
		const jsonInput = JSON.stringify(msg);
		const t0 = performance.now();
		const jsonOutput = wasmModule.process_message(jsonInput);
		const elapsed = performance.now() - t0;
		if (elapsed > 100) {
			console.log(`[worker] process_message(${msg.type}) took ${(elapsed / 1000).toFixed(2)}s`);
		}
		return JSON.parse(jsonOutput);
	} catch (err) {
		// Detect WASM module death — RuntimeError or "unreachable" means the
		// module's memory is corrupted and cannot accept further calls.
		if (err instanceof WebAssembly.RuntimeError ||
			err.message?.includes('unreachable')) {
			console.error('WASM module crashed, marking for restart:', err.message);
			wasmModule = null;
			return {
				type: 'Error',
				message: `Engine crashed: ${err.message}. Restarting...`,
				feature_id: null,
				needsRestart: true
			};
		}
		return {
			type: 'Error',
			message: `Engine error: ${err.message}`,
			feature_id: null
		};
	}
}

/**
 * Collect mesh data for features as Transferable typed arrays.
 *
 * Uses the engine's `get_renderable_feature_indices()` to determine which
 * features should render. Features consumed by a successful boolean union
 * are excluded (their geometry is merged into the consuming feature).
 * When union fails, both features render (multi-body fallback).
 */
function collectMeshes() {
	if (!wasmModule) return { meshes: [], transferables: [] };

	const meshes = [];
	const transferables = [];

	const features = JSON.parse(wasmModule.get_feature_tree()).features || [];

	// Get the set of renderable feature indices from the engine.
	// This excludes features consumed by successful boolean operations.
	let renderableSet;
	if (wasmModule.get_renderable_feature_indices) {
		const renderableArr = wasmModule.get_renderable_feature_indices();
		renderableSet = new Set(renderableArr);
	} else {
		// Fallback: render only the last mesh-producing feature (old behavior)
		renderableSet = null;
	}

	for (let i = 0; i < features.length; i++) {
		const vertView = wasmModule.get_mesh_vertices(i);
		const normView = wasmModule.get_mesh_normals(i);
		const idxView = wasmModule.get_mesh_indices(i);

		if (vertView.length === 0) continue;

		// Skip features not in the renderable set
		if (renderableSet !== null) {
			if (!renderableSet.has(i)) continue;
		}

		const vertices = new Float32Array(vertView);
		const normals = new Float32Array(normView);
		const indices = new Uint32Array(idxView);

		// Get face range data with GeomRef enrichment
		let faceRanges = [];
		try {
			const faceDataJson = wasmModule.get_face_data(i);
			faceRanges = JSON.parse(faceDataJson);
		} catch (e) {
			console.warn('Face data unavailable for feature', i, e);
		}

		// Get edge data for edge overlay rendering
		let edges = null;
		try {
			if (wasmModule.get_edge_vertices && wasmModule.get_edge_data) {
				const edgeVertView = wasmModule.get_edge_vertices(i);
				if (edgeVertView.length > 0) {
					const edgeVertices = new Float32Array(edgeVertView);
					const edgeDataJson = wasmModule.get_edge_data(i);
					const edgeRanges = JSON.parse(edgeDataJson);
					edges = { vertices: edgeVertices, ranges: edgeRanges };
					transferables.push(edgeVertices.buffer);
				}
			}
		} catch (e) {
			console.warn('Edge data unavailable for feature', i, e);
		}

		meshes.push({
			featureIndex: i,
			featureId: features[i].id,
			vertices,
			normals,
			indices,
			triangleCount: indices.length / 3,
			faceRanges,
			edges
		});

		transferables.push(vertices.buffer, normals.buffer, indices.buffer);
	}

	return { meshes, transferables };
}

self.onmessage = async function (event) {
	const msg = event.data;

	if (msg.type === 'init') {
		basePath = msg.basePath || '';
		await initEngine(msg.wasmUrl);
		return;
	}

	const response = processMessage(msg);

	// If the WASM module crashed, try to auto-restart before responding.
	// The wasm-bindgen init function caches the internal `wasm` variable and
	// returns early if set, so re-using `import()` + `default()` won't create
	// a new instance after a crash. We must import a FRESH copy of the JS
	// module via blob URL to bypass the module cache and get a clean `wasm`
	// variable, then re-fetch and instantiate the WASM binary.
	if (response.needsRestart && lastWasmUrl) {
		console.log('Auto-restarting WASM module after crash...');
		try {
			// Fetch the JS module as text and create a blob URL for a fresh import
			const jsResp = await fetch(lastWasmUrl);
			const jsText = await jsResp.text();
			const blob = new Blob([jsText], { type: 'text/javascript' });
			const blobUrl = URL.createObjectURL(blob);
			const freshWasm = await import(/* @vite-ignore */ blobUrl);
			URL.revokeObjectURL(blobUrl);
			// Initialize with the default URL (will fetch .wasm relative to blob)
			// Pass explicit wasm URL since blob URL can't resolve relative paths
			const wasmBinaryUrl = lastWasmUrl.replace(/\.js$/, '_bg.wasm');
			await freshWasm.default(wasmBinaryUrl);
			freshWasm.init();
			wasmModule = freshWasm;
			console.log('WASM module restarted successfully');
			response.message = `Engine recovered: ${response.message}`;
			response.needsRestart = false;
		} catch (restartErr) {
			console.error('WASM module restart failed:', restartErr.message);
		}
	}

	// Adapt Rust SketchSolved (nested) → store format (flat)
	if (response.type === 'SketchSolved' && response.solved) {
		const s = response.solved;
		const statusType = s.status?.type || 'SolveFailed';
		let flatStatus, dof, failed;
		switch (statusType) {
			case 'FullyConstrained': flatStatus = 'ok'; dof = 0; failed = []; break;
			case 'UnderConstrained': flatStatus = 'under_constrained'; dof = s.status.dof ?? -1; failed = []; break;
			case 'OverConstrained': flatStatus = 'over_constrained'; dof = -1; failed = s.status.conflicts || []; break;
			default: flatStatus = 'error'; dof = -1; failed = []; break;
		}
		self.postMessage({
			type: 'SketchSolved',
			positions: s.positions || {},
			solvedRadii: s.radii || {},
			status: flatStatus,
			dof,
			failed,
			profiles: s.profiles || []
		});
		return;
	}

	if (response.type === 'ModelUpdated') {
		const t1 = performance.now();
		const { meshes, transferables } = collectMeshes();
		const meshElapsed = performance.now() - t1;
		if (meshElapsed > 50) {
			console.log(`[worker] collectMeshes took ${(meshElapsed / 1000).toFixed(2)}s`);
		}
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
