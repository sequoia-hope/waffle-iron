/**
 * Waffle Iron Engine Web Worker
 *
 * Loads the Rust WASM module and processes messages from the main thread.
 * All engine computation happens in this worker to keep the UI responsive.
 *
 * Message protocol:
 *   Main → Worker: { type: "init" } | UiToEngine JSON
 *   Worker → Main: { type: "ready" } | { type: "error", message } | EngineToUi JSON
 *
 * Mesh data is transferred as Transferable ArrayBuffers for zero-copy performance.
 */

let wasmModule = null;

/**
 * Initialize the WASM module.
 * @param {string} wasmUrl - URL to the wasm_bridge.js module
 */
async function initEngine(wasmUrl) {
  try {
    // Dynamic import of the wasm-bindgen generated JS glue
    const wasm = await import(wasmUrl || './pkg/wasm_bridge.js');
    await wasm.default(); // Initialize WASM module
    wasm.init();          // Set up panic hooks + engine state
    wasmModule = wasm;

    self.postMessage({ type: 'ready' });
  } catch (err) {
    self.postMessage({
      type: 'Error',
      message: `WASM initialization failed: ${err.message}`,
      feature_id: null,
    });
  }
}

/**
 * Process a UiToEngine message and return the EngineToUi response.
 */
function processMessage(msg) {
  if (!wasmModule) {
    return {
      type: 'Error',
      message: 'Engine not initialized',
      feature_id: null,
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
      feature_id: null,
    };
  }
}

/**
 * Collect mesh data for all features as Transferable typed arrays.
 *
 * Returns an array of { vertices, normals, indices } objects where each
 * array is a copy from WASM memory (required since WASM views are invalidated
 * by memory growth). The ArrayBuffers are listed as transferables for
 * zero-copy postMessage transfer.
 */
function collectMeshes() {
  if (!wasmModule) return { meshes: [], transferables: [] };

  const meshCount = wasmModule.get_mesh_count();
  const meshes = [];
  const transferables = [];

  const features = JSON.parse(wasmModule.get_feature_tree()).features || [];

  for (let i = 0; i < features.length; i++) {
    // Get typed array views into WASM memory (must copy immediately)
    const vertView = wasmModule.get_mesh_vertices(i);
    const normView = wasmModule.get_mesh_normals(i);
    const idxView = wasmModule.get_mesh_indices(i);

    if (vertView.length === 0) continue;

    // Copy from WASM memory into standalone ArrayBuffers
    const vertices = new Float32Array(vertView);
    const normals = new Float32Array(normView);
    const indices = new Uint32Array(idxView);

    meshes.push({
      featureIndex: i,
      featureId: features[i].id,
      vertices,
      normals,
      indices,
      triangleCount: indices.length / 3,
    });

    // Mark buffers for zero-copy transfer
    transferables.push(vertices.buffer, normals.buffer, indices.buffer);
  }

  return { meshes, transferables };
}

// ── Message Handler ──────────────────────────────────────────────────

self.onmessage = async function (event) {
  const msg = event.data;

  if (msg.type === 'init') {
    await initEngine(msg.wasmUrl);
    return;
  }

  const response = processMessage(msg);

  // For ModelUpdated responses, attach mesh data as typed arrays
  if (response.type === 'ModelUpdated') {
    const { meshes, transferables } = collectMeshes();
    response.meshes = meshes;
    // Remove JSON-serialized mesh data (use typed arrays instead)
    self.postMessage(response, transferables);
  } else {
    self.postMessage(response);
  }
};

// ── Error Handler ────────────────────────────────────────────────────

self.onerror = function (error) {
  self.postMessage({
    type: 'Error',
    message: `Worker error: ${error.message || error}`,
    feature_id: null,
  });
};
