/**
 * Waffle Iron Engine Bridge
 *
 * Main-thread API for communicating with the WASM engine running in a Web Worker.
 * Provides a Promise-based interface for sending commands and receiving results.
 *
 * Usage:
 *   import { EngineBridge } from './bridge.js';
 *
 *   const bridge = new EngineBridge();
 *   await bridge.init();
 *
 *   const result = await bridge.send({ type: 'AddFeature', operation: { ... } });
 *   console.log(result); // EngineToUi response
 */

export class EngineBridge {
  constructor() {
    this._worker = null;
    this._pendingCallbacks = [];
    this._onModelUpdated = null;
    this._onSketchSolved = null;
    this._onSelectionChanged = null;
    this._onHoverChanged = null;
    this._onError = null;
  }

  /**
   * Initialize the engine worker.
   * @param {string} workerUrl - URL to worker.js
   * @param {string} wasmUrl - URL to the wasm_bridge.js module (passed to worker)
   * @returns {Promise<void>} Resolves when the engine is ready.
   */
  init(workerUrl = './worker.js', wasmUrl = './pkg/wasm_bridge.js') {
    return new Promise((resolve, reject) => {
      this._worker = new Worker(workerUrl, { type: 'module' });

      const onReady = (event) => {
        const msg = event.data;
        if (msg.type === 'ready') {
          this._worker.removeEventListener('message', onReady);
          this._worker.addEventListener('message', (e) => this._handleMessage(e));
          resolve();
        } else if (msg.type === 'Error') {
          reject(new Error(msg.message));
        }
      };

      this._worker.addEventListener('message', onReady);
      this._worker.addEventListener('error', (e) => {
        reject(new Error(`Worker failed to load: ${e.message}`));
      });

      this._worker.postMessage({ type: 'init', wasmUrl });
    });
  }

  /**
   * Send a UiToEngine command and get the response.
   * @param {object} message - UiToEngine message (must have a `type` field)
   * @returns {Promise<object>} EngineToUi response
   */
  send(message) {
    return new Promise((resolve, reject) => {
      if (!this._worker) {
        reject(new Error('Bridge not initialized. Call init() first.'));
        return;
      }

      this._pendingCallbacks.push({ resolve, reject });
      this._worker.postMessage(message);
    });
  }

  /**
   * Register event handlers for asynchronous engine events.
   * @param {string} event - Event name: 'modelUpdated', 'sketchSolved', 'selectionChanged', 'hoverChanged', 'error'
   * @param {function} callback - Event handler
   */
  on(event, callback) {
    switch (event) {
      case 'modelUpdated':
        this._onModelUpdated = callback;
        break;
      case 'sketchSolved':
        this._onSketchSolved = callback;
        break;
      case 'selectionChanged':
        this._onSelectionChanged = callback;
        break;
      case 'hoverChanged':
        this._onHoverChanged = callback;
        break;
      case 'error':
        this._onError = callback;
        break;
    }
  }

  /**
   * Shut down the worker.
   */
  terminate() {
    if (this._worker) {
      this._worker.terminate();
      this._worker = null;
    }
  }

  // ── Internal ─────────────────────────────────────────────────────

  _handleMessage(event) {
    const msg = event.data;
    const pending = this._pendingCallbacks.shift();

    // Route to event handlers
    switch (msg.type) {
      case 'ModelUpdated':
        if (this._onModelUpdated) this._onModelUpdated(msg);
        break;
      case 'SketchSolved':
        if (this._onSketchSolved) this._onSketchSolved(msg);
        break;
      case 'SelectionChanged':
        if (this._onSelectionChanged) this._onSelectionChanged(msg);
        break;
      case 'HoverChanged':
        if (this._onHoverChanged) this._onHoverChanged(msg);
        break;
      case 'Error':
        if (this._onError) this._onError(msg);
        break;
    }

    // Resolve the pending promise
    if (pending) {
      if (msg.type === 'Error') {
        pending.reject(new Error(msg.message));
      } else {
        pending.resolve(msg);
      }
    }
  }
}
