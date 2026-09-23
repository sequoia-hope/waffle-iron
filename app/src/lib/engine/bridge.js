/**
 * Waffle Iron Engine Bridge (SvelteKit version)
 *
 * Main-thread API for communicating with the WASM engine running in a Web Worker.
 * Provides a Promise-based interface for sending commands and receiving results.
 */

import { log } from './logger.js';

/**
 * Pointer feedback the engine answers without touching the model. These never
 * take the engine lock (specs/waffle_mcp_server.md §2.7: selection and hover
 * stay live during an agent call); the request id on every send is what keeps
 * them safe, since they overtake nothing and answer only themselves.
 */
const UNGATED_TYPES = new Set(['HoverEntity', 'SelectEntity']);

export class EngineBridge {
	constructor() {
		/** @type {Worker | null} */
		this._worker = null;
		/**
		 * In-flight sends by request id. The worker echoes the id it was given,
		 * so an answer reaches its own caller whatever order answers arrive in.
		 * @type {Map<number, {resolve: Function, reject: Function, entry?: object | null}>}
		 */
		this._pending = new Map();
		/** Monotonic request id. Starts at 1, so a missing id is falsy. */
		this._nextRequestId = 1;
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
		/**
		 * Wraps every gated send: `(message, post) => Promise<response>`. The
		 * store installs the engine lock here.
		 * @type {((message: object, post: () => Promise<object>) => Promise<object>) | null}
		 */
		this._sendGate = null;
		/**
		 * @type {Array<{type: string, origin: 'user' | 'agent' | 'pointer', t: number, message?: object, response?: {type: string, feature_id: string | null}}> | null}
		 */
		this._sendLog = null;
		this._logPayloads = false;
	}

	/**
	 * Initialize the engine worker.
	 * @param {string} wasmUrl - URL to the wasm_bridge.js module (passed to worker)
	 * @returns {Promise<void>} Resolves when the engine is ready.
	 */
	init(wasmUrl) {
		return new Promise((resolve, reject) => {
			log('system', 'Creating engine worker');
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
					log('system', 'Engine worker ready');
					resolve();
				} else if (msg.type === 'Error') {
					log('error', `Worker init error: ${msg.message}`);
					reject(new Error(msg.message));
				}
			};

			this._worker.addEventListener('message', onReady);
			this._worker.addEventListener('error', (e) => {
				log('error', `Worker load error: ${e.message}`);
				reject(new Error(`Worker failed to load: ${e.message}`));
			});

			const basePath = wasmUrl.substring(0, wasmUrl.indexOf('/pkg/'));
			this._worker.postMessage({ type: 'init', wasmUrl, basePath });
		});
	}

	/**
	 * Install the send gate (the store's engine lock). Hover/select bypass it.
	 * @param {((message: object, post: () => Promise<object>) => Promise<object>) | null} gate
	 */
	setSendGate(gate) {
		this._sendGate = gate;
	}

	/**
	 * Send a UiToEngine command and get the response. Goes through the send
	 * gate, so a user-originated message waits while an agent call holds the
	 * engine lock.
	 * @param {object} message - UiToEngine message (must have a `type` field)
	 * @returns {Promise<object>} EngineToUi response
	 */
	send(message) {
		if (UNGATED_TYPES.has(message.type)) return this._post(message, 'pointer');
		if (this._sendGate) return this._sendGate(message, () => this._post(message, 'user'));
		return this._post(message, 'user');
	}

	/**
	 * Agent-link path: post without the gate. The caller must already hold the
	 * engine lock (store `sendAgentMessage`).
	 * @param {object} message
	 * @returns {Promise<object>}
	 */
	sendUngated(message) {
		return this._post(message, 'agent');
	}

	/**
	 * Start (true, clearing the log) or stop (false) recording every send with
	 * its origin — the agent-link oracles: no interleaving (O7) and, with
	 * `payloads`, each message and its answer's type and feature id (O3 parity).
	 * @param {boolean} on
	 * @param {{ payloads?: boolean }} [opts]
	 */
	recordSends(on, { payloads = false } = {}) {
		this._sendLog = on ? [] : null;
		this._logPayloads = on && payloads;
	}

	/** @returns {Array<{type: string, origin: 'user' | 'agent' | 'pointer', t: number, message?: object, response?: object}>} */
	getSendLog() {
		return this._sendLog ? this._sendLog.map((entry) => ({ ...entry })) : [];
	}

	/**
	 * @param {object} message
	 * @param {'user' | 'agent' | 'pointer'} origin
	 * @returns {Promise<object>}
	 */
	_post(message, origin) {
		return new Promise((resolve, reject) => {
			if (!this._worker) {
				reject(new Error('Bridge not initialized. Call init() first.'));
				return;
			}

			log('engine', `Send: ${message.type}`, { type: message.type });

			const id = this._nextRequestId++;
			try {
				this._worker.postMessage({ id, msg: message });
				/** @type {any} */
				let entry = null;
				if (this._sendLog) {
					entry = { type: message.type, origin, t: performance.now() };
					if (this._logPayloads) entry.message = JSON.parse(JSON.stringify(message));
					this._sendLog.push(entry);
				}
				this._pending.set(id, { resolve, reject, entry });
			} catch (err) {
				log('error', `postMessage failed: ${err}`);
				reject(err);
			}
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
			case 'progress':
				this._onProgress = callback;
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
	 * Build the rejection for an engine `Error` response. Typed error fields
	 * (ICR-2): `kind` is the engine's ErrorKind when the failure is an engine
	 * error; absent for bridge-level failures.
	 * @param {any} msg
	 * @returns {Error & {kind: object | null, featureId: string | null, needsRestart: boolean}}
	 */
	_engineError(msg) {
		const err = /** @type {Error & {kind: object | null, featureId: string | null, needsRestart: boolean}} */ (
			new Error(msg.message)
		);
		err.kind = msg.kind ?? null;
		err.featureId = msg.feature_id ?? null;
		err.needsRestart = msg.needsRestart === true;
		return err;
	}

	/**
	 * @param {MessageEvent} event
	 */
	_handleMessage(event) {
		const frame = event.data;
		// `{id, msg}` answers one send. A bare message is unsolicited — the
		// worker's `self.onerror` — and answers none.
		const envelope = !!frame && typeof frame === 'object' && 'msg' in frame;
		const msg = envelope ? frame.msg : frame;
		const id = envelope ? frame.id : null;
		// A rebuild progress frame (`specs/b4_balanced_union.md` §2.3) is
		// unsolicited too: it arrives WHILE the command it belongs to is still
		// computing, and answers nothing. Not logged per frame (a long union
		// posts many).
		if (!envelope && msg?.type === 'Progress') {
			if (this._onProgress) this._onProgress(msg);
			return;
		}
		const pending = id ? this._pending.get(id) : null;
		if (pending) this._pending.delete(id);
		if (pending?.entry) {
			// An authoring tool answers with a `ToolResult`, which carries the
			// id of any feature it created in the MCP payload rather than at
			// the top level (S3 C4). O3 replay learns recorded-id → fresh-id
			// from this field, so without the second lookup every step after a
			// `Tool` that created a feature replays against a stale id — and
			// the replay swallows the resulting failure, losing the step.
			const featureId = msg.feature_id ?? msg.structuredContent?.feature_id ?? null;
			pending.entry.response = { type: msg.type, feature_id: featureId };
		}

		// Build summary data for the log entry
		const summary = { type: msg.type };
		if (msg.type === 'ModelUpdated') summary.meshCount = msg.meshes?.length ?? 0;
		if (msg.type === 'SketchSolved') {
			const s = msg.solved || msg;
			const st = s.status || {};
			summary.dof = st.dof ?? msg.dof ?? -1;
			summary.status = st.type || (typeof st === 'string' ? st : 'unknown');
		}
		if (msg.type === 'Error') summary.message = msg.message;
		if (msg.needsRestart) summary.needsRestart = true;
		log('engine', `Recv: ${msg.type}`, summary);

		// If the worker signals a crash recovery, log it prominently and
		// notify the error handler so the store can reset engineReady.
		if (msg.needsRestart) {
			log('error', 'Engine crashed and could not auto-restart. Subsequent operations may fail.');
			if (this._onError) this._onError({ message: 'Engine crashed', needsRestart: true });
		}

		switch (msg.type) {
			case 'ModelUpdated':
				if (this._onModelUpdated) this._onModelUpdated(msg);
				break;
			case 'ToolResult':
				// An authoring tool runs inside the engine now (S3 C4), so its
				// answer is not a `ModelUpdated` — but the document changed.
				// The model it carries goes to the same handler, so the tree,
				// meshes, errors and autosave refresh exactly as for any other
				// step. A read-only tool carries none.
				if (msg.model && this._onModelUpdated) this._onModelUpdated(msg.model);
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
			case 'GearPreviewGenerated':
			case 'GearProfileGenerated':
			case 'SprocketPreviewGenerated':
			case 'SprocketProfileGenerated':
			case 'PlanetaryGenerated':
			case 'PlanetaryPreviewGenerated':
				// Resolved via pending promise — no event dispatch needed
				break;
		}

		if (pending) {
			if (msg.type === 'Error') pending.reject(this._engineError(msg));
			else pending.resolve(msg);
		} else if (msg.type === 'Error') {
			// An Error answering no request is the worker's `self.onerror`: an
			// uncaught failure outside `processMessage`, which leaves the worker
			// unable to answer anything still in flight. FIFO pairing used to
			// reject whichever send happened to be oldest and hang the rest;
			// fail them all with the one error that actually happened.
			const err = this._engineError(msg);
			for (const p of this._pending.values()) p.reject(err);
			this._pending.clear();
		}
	}
}
