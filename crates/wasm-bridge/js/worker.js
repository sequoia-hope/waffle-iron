/**
 * Waffle Iron Engine Web Worker
 *
 * Loads the Rust WASM module and processes messages from the main thread.
 * All engine computation happens in this worker to keep the UI responsive.
 *
 * Message protocol:
 *   Main → Worker: { type: "init" } | UiToEngine JSON
 *   Worker → Main: { type: "ready" } | { type: "error", message } | EngineToUi JSON
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
 * Get the feature tree without sending a command.
 */
function getFeatureTree() {
  if (!wasmModule) return null;
  try {
    return JSON.parse(wasmModule.get_feature_tree());
  } catch {
    return null;
  }
}

/**
 * Get mesh JSON for a feature by index.
 */
function getMeshJson(featureIndex) {
  if (!wasmModule) return null;
  try {
    return JSON.parse(wasmModule.get_mesh_json(featureIndex));
  } catch {
    return null;
  }
}

// ── Message Handler ──────────────────────────────────────────────────

self.onmessage = async function (event) {
  const msg = event.data;

  if (msg.type === 'init') {
    await initEngine(msg.wasmUrl);
    return;
  }

  const response = processMessage(msg);

  // Transfer mesh data efficiently if present
  // (Future: use Transferable typed arrays instead of JSON for meshes)
  self.postMessage(response);
};

// ── Error Handler ────────────────────────────────────────────────────

self.onerror = function (error) {
  self.postMessage({
    type: 'Error',
    message: `Worker error: ${error.message || error}`,
    feature_id: null,
  });
};
