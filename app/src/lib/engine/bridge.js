/**
 * Waffle Iron Engine Bridge (SvelteKit version)
 *
 * Main-thread API for communicating with the WASM engine running in a Web Worker.
 * Provides a Promise-based interface for sending commands and receiving results.
 */

export class EngineBridge {
	constructor() {
		/** @type {Worker | null} */
		this._worker = null;
		/** @type {Array<{resolve: Function, reject: Function}>} */
		this._pendingCallbacks = [];
		/** @type {Function | null} */
		this._onModelUpdated = null;
		/** @type {Function | null} */
		this._onSketchSolved = null;
		/** @type {Function | null} */
		this._onSelectionChanged = null;
		/** @type {Function | null} */
		this._onHoverChanged = null;
		/** @type {Function | null} */
		this._onError = null;
	}

	/**
	 * Initialize the engine worker.
	 * @param {string} wasmUrl - URL to the wasm_bridge.js module (passed to worker)
	 * @returns {Promise<void>} Resolves when the engine is ready.
	 */
	init(wasmUrl = '/pkg/wasm_bridge.js') {
		return new Promise((resolve, reject) => {
			this._worker = new Worker(
				new URL('./worker.js', import.meta.url),
				{ type: 'module' }
			);

			/** @param {MessageEvent} event */
			const onReady = (event) => {
				const msg = event.data;
				if (msg.type === 'ready') {
					this._worker?.removeEventListener('message', onReady);
					this._worker?.addEventListener('message', (e) => this._handleMessage(e));
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
	 * @param {string} event - Event name
	 * @param {Function} callback - Event handler
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

	/**
	 * @param {MessageEvent} event
	 */
	_handleMessage(event) {
		const msg = event.data;
		const pending = this._pendingCallbacks.shift();

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

		if (pending) {
			if (msg.type === 'Error') {
				pending.reject(new Error(msg.message));
			} else {
				pending.resolve(msg);
			}
		}
	}
}
