/**
 * Waffle Iron Engine Web Worker (SvelteKit version)
 *
 * Loads the Rust WASM module and processes messages from the main thread.
 * The constraint solver is pure Rust (Levenberg-Marquardt), compiled into
 * the same WASM module — no separate Emscripten/slvS module needed.
 * All engine computation happens in this worker to keep the UI responsive.
 */

let wasmModule = null;
let basePath = '';

/**
 * The wasm instance's raw exports (what `__wbg_finalize_init` returns), kept
 * only for `exports.memory` — the `WebAssembly.Memory` object survives a trap,
 * so its size is readable afterwards and is what distinguishes an OUT-OF-MEMORY
 * abort from an ordinary panic. See `classifyTrap`.
 */
let wasmExports = null;

/**
 * wasm32 is a 32-bit address space: 4 GiB is the architectural ceiling, and
 * browsers cap a single memory below it. Past this mark a trap is
 * overwhelmingly an allocation failure rather than a logic panic.
 */
const OOM_SUSPECT_BYTES = 1024 * 1024 * 1024; // 1 GiB

/** Current wasm heap size in bytes, or 0 if unavailable. */
function wasmHeapBytes() {
	try {
		return wasmExports?.memory?.buffer?.byteLength ?? 0;
	} catch {
		return 0; // buffer detached mid-growth
	}
}

function formatGiB(bytes) {
	return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/**
 * Turn a wasm trap into a message that says what actually happened.
 *
 * A trap reaches JS as a bare `unreachable` with no context. Two very different
 * causes produce it:
 *
 *  - a Rust `panic!`, which DOES run the `init()` panic hook and therefore
 *    prints a `WASM PANIC: ...` line to this worker's console; and
 *  - an allocation failure, which calls `alloc::handle_alloc_error` ->
 *    `abort()`. That path is NOT a panic, so no hook runs and no message is
 *    printed — historically indistinguishable from a logic bug.
 *
 * The heap size at trap time separates them. Measured case: assay R0088 needs
 * ~6.8 GB peak (verified natively, where it completes successfully) and so
 * cannot fit in wasm32 at all.
 *
 * This is a classification, not a certainty, and the message says so rather
 * than asserting a cause it cannot prove.
 */
function classifyTrap(rawMessage) {
	const bytes = wasmHeapBytes();
	if (bytes >= OOM_SUSPECT_BYTES) {
		return {
			message:
				`Out of memory: this model needs more than the browser engine can address. ` +
				`The engine had grown to ${formatGiB(bytes)} when it failed; WASM is 32-bit and ` +
				`cannot exceed 4 GB. The same model may still build in a native (64-bit) run. ` +
				`(underlying trap: ${rawMessage})`,
			oom: true,
			heapBytes: bytes
		};
	}
	return {
		message:
			`Engine crashed: ${rawMessage}. Restarting... ` +
			`(heap ${formatGiB(bytes)}; check this worker's console for a "WASM PANIC:" line — ` +
			`if there is none, the cause was not a Rust panic)`,
		oom: false,
		heapBytes: bytes
	};
}

/**
 * Initialize the WASM module.
 * @param {string} wasmUrl - URL to the wasm_bridge.js module
 */
async function initEngine(wasmUrl) {
	try {
		lastWasmUrl = wasmUrl;
		const wasm = await import(/* @vite-ignore */ wasmUrl);
		// `__wbg_finalize_init` returns the instance exports; hold them for
		// `memory` (trap classification), not for calling into.
		wasmExports = await wasm.default();
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
			const verdict = classifyTrap(err.message);
			console.error(
				`WASM module crashed (heap ${formatGiB(verdict.heapBytes)}, ` +
				`${verdict.oom ? 'OUT OF MEMORY' : 'cause unclassified'}), marking for restart:`,
				err.message
			);
			wasmModule = null;
			return {
				type: 'Error',
				message: verdict.message,
				feature_id: null,
				needsRestart: true,
				oom: verdict.oom
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
 * Collect mesh data per body (per mesh-bearing output) as Transferable typed
 * arrays. Each body is one entry; multi-body features contribute one entry per
 * output. Uses the engine's per-body accessors, which already exclude features
 * consumed by a successful boolean.
 */
function collectBodies() {
	const meshes = [];
	const transferables = [];

	const count = wasmModule.get_body_count();
	let metadata = [];
	try {
		metadata = JSON.parse(wasmModule.get_body_metadata());
	} catch (e) {
		console.warn('Body metadata unavailable', e);
	}

	for (let b = 0; b < count; b++) {
		const vertView = wasmModule.get_body_vertices(b);
		if (vertView.length === 0) continue;
		const normView = wasmModule.get_body_normals(b);
		const idxView = wasmModule.get_body_indices(b);

		const vertices = new Float32Array(vertView);
		const normals = new Float32Array(normView);
		const indices = new Uint32Array(idxView);

		let faceRanges = [];
		try {
			faceRanges = JSON.parse(wasmModule.get_body_face_data(b));
		} catch (e) {
			console.warn('Face data unavailable for body', b, e);
		}

		let edges = null;
		try {
			if (wasmModule.get_body_edge_vertices && wasmModule.get_body_edge_data) {
				const edgeVertView = wasmModule.get_body_edge_vertices(b);
				if (edgeVertView.length > 0) {
					const edgeVertices = new Float32Array(edgeVertView);
					const edgeRanges = JSON.parse(wasmModule.get_body_edge_data(b));
					edges = { vertices: edgeVertices, ranges: edgeRanges };
					transferables.push(edgeVertices.buffer);
				}
			}
		} catch (e) {
			console.warn('Edge data unavailable for body', b, e);
		}

		const meta = metadata[b] || {};
		meshes.push({
			bodyIndex: b,
			// Persistent body identity "{featureId}/{outputKey.tag()}" + resolved
			// display name, both from the engine (authoritative).
			bodyId: meta.bodyId ?? `${meta.featureId}/Main`,
			name: meta.name ?? null,
			featureIndex: meta.featureIndex,
			featureId: meta.featureId,
			outputKey: meta.outputKey ?? null,
			outputIndex: meta.outputIndex ?? 0,
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

/**
 * Collect mesh data for features as Transferable typed arrays.
 *
 * Uses the engine's `get_renderable_feature_indices()` to determine which
 * features should render. Features consumed by a successful boolean union
 * are excluded (their geometry is merged into the consuming feature).
 * When union fails, both features render (multi-body fallback).
 *
 * Legacy fallback for bundles without the per-body accessors.
 */
function collectMeshes() {
	if (!wasmModule) return { meshes: [], transferables: [] };

	// Preferred path: per-body accessors. A feature can emit multiple bodies
	// (e.g. a boolean split); these address each mesh-bearing output separately
	// so every body renders. Older bundles without these fall through below.
	if (wasmModule.get_body_count && wasmModule.get_body_metadata) {
		return collectBodies();
	}

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
			bodyIndex: meshes.length,
			bodyId: `${features[i].id}/Main`,
			name: null,
			featureIndex: i,
			featureId: features[i].id,
			outputKey: null,
			outputIndex: 0,
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
			wasmExports = await freshWasm.default(wasmBinaryUrl);
			freshWasm.init();
			wasmModule = freshWasm;
			console.log('WASM module restarted successfully');
			// An OOM message already states the cause and the limit; burying it
			// behind "Engine recovered:" would hide the actionable part. Say the
			// engine restarted, but keep the diagnosis first.
			response.message = response.oom
				? `${response.message} The engine has been restarted.`
				: `Engine recovered: ${response.message}`;
			response.needsRestart = false;
		} catch (restartErr) {
			console.error('WASM module restart failed:', restartErr.message);
		}
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
