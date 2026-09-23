/**
 * The viewer side of `waffle-viewer/1` (specs/waffle_server_mode.md §4): a
 * `/view` page attaches to a relay running `--kernel host`, holds no
 * authoritative state, and draws what the host computes.
 *
 * - `attach{code, encodings}` from the link `waffle_connect` handed out, or
 *   `attach{session, have}` on a reload / reconnect. `have` names the state
 *   this browser already holds; a host in the same state sends only
 *   `welcome`. The session token is re-minted on every heartbeat (`session`
 *   frames), so the resume window runs from this tab's last sign of life and
 *   survives a relay restart.
 * - Every `snapshot` is the whole document minus the geometry; an `update`
 *   is the keys that changed since the state this viewer holds (§4.3), and
 *   is merged onto it. The bodies either frame names are fetched by content
 *   id (`want` → binary `blob` frames), from the cache first.
 * - `rebuild` frames say what the engine is on, so the page can show the
 *   feature by name instead of a frozen model.
 * - `capture_request` / `view_request` are answered from this tab's viewport
 *   (the same window events the editor's agent tools use), and `select`
 *   carries this viewer's picks back, so `viewport_*` and `selection_get`
 *   work over the link with no engine in the browser.
 * - The last snapshot and every blob go into IndexedDB, so a tab iOS killed
 *   paints the model at once when it reloads (§4.6) and asks the host for
 *   nothing it already has.
 * - Reconnect policy as the agent link's (§4.6): at once on visibility /
 *   pageshow / online, else backoff while visible, never while hidden; a
 *   socket that looks open is probed with `ping` on wake.
 *
 * A MODULE-LEVEL singleton, like `$lib/agent/link.js`.
 */
import { get, writable } from 'svelte/store';
import { applyViewerSnapshot } from '$lib/engine/store.svelte.js';
import { blobGet, blobPut, snapshotGet, snapshotPut } from './cache.js';
import { compactSupported, decodeArrays, decodeBlobFrame, meshFromEntry } from './decode.js';

export const VIEWER_PROTOCOL = 'waffle-viewer/1';

const RECONNECT_FIRST_MS = 1000;
const RECONNECT_MAX_MS = 30000;
const PROBE_TIMEOUT_MS = 5000;
/** `bye` reasons after which the stored session is useless. */
const TERMINAL_BYE = new Set(['invalid_code', 'session_expired', 'protocol_mismatch']);

/**
 * @typedef {{
 *   state: 'idle' | 'connecting' | 'attached' | 'reconnecting' | 'failed',
 *   host: string | null,
 *   viewerId: string | null,
 *   epoch: string | null,
 *   revision: number | null,
 *   documentName: string | null,
 *   bodies: number,
 *   stale: boolean,
 *   encoding: string | null,
 *   rebuilding: { tool: string, feature: string | null, message: string | null, elapsedMs: number } | null,
 *   agent: string | null,
 *   viewers: number,
 *   reason: string | null,
 * }} ViewerStatus
 */

/** @type {import('svelte/store').Writable<ViewerStatus>} */
export const viewerLink = writable({
	state: 'idle',
	host: null,
	viewerId: null,
	epoch: null,
	revision: null,
	documentName: null,
	bodies: 0,
	stale: false,
	encoding: null,
	rebuilding: null,
	agent: null,
	viewers: 0,
	reason: null
});

/** Counters the tests read: what was fetched, what the cache answered. */
export const viewerStats = {
	snapshots: 0,
	updates: 0,
	blobRequests: 0,
	blobsReceived: 0,
	cacheHits: 0,
	cachedPaint: false,
	bytesReceived: 0,
	// `rebuild` frames seen (§4.3). A fast tool's banner can come and go
	// between two polls, so what it did is counted, not watched.
	rebuildsStarted: 0,
	rebuildsDone: 0,
	lastRebuildTool: null
};

/** @type {WebSocket | null} */
let socket = null;
/** @type {string | null} */
let hostUrl = null;
/** The blobs fetched this session, by id: the memory tier of the cache. */
const memory = new Map();
/** Decoded arrays by mesh id, so an update that changes no geometry re-decodes nothing. */
const decoded = new Map();
/** Blob requests in flight, by id. */
const wanted = new Map();
/** The full state this viewer holds: the last snapshot with every update merged onto it. */
let held = null;
/** The blob encoding this viewer asked for and the host agreed to. */
let encoding = 'raw/1';
/** @type {ReturnType<typeof setTimeout> | null} */
let reconnectTimer = null;
let reconnectDelayMs = RECONNECT_FIRST_MS;
/** @type {ReturnType<typeof setTimeout> | null} */
let probeTimer = null;
let lifecycleInstalled = false;
/** The snapshot being rendered, so a newer one supersedes it. */
let renderSerial = 0;

function storageKey(host) {
	return `waffle-viewer:${host}`;
}

/** @param {string} host @returns {{ session: string, epoch: string | null, revision: number | null } | null} */
function readStored(host) {
	for (const storage of [sessionStorage, localStorage]) {
		try {
			const raw = storage.getItem(storageKey(host));
			if (raw) return JSON.parse(raw);
		} catch {
			// unavailable
		}
	}
	return null;
}

/** @param {string} host @param {{ session: string | null, epoch: string | null, revision: number | null } | null} value */
function writeStored(host, value) {
	// Both (§4.6): an iOS tab discard does not reliably keep sessionStorage,
	// and a viewer holds no authority that another tab could misuse.
	for (const storage of [sessionStorage, localStorage]) {
		try {
			if (value) storage.setItem(storageKey(host), JSON.stringify(value));
			else storage.removeItem(storageKey(host));
		} catch {
			// unavailable
		}
	}
}

function update(patch) {
	viewerLink.update((s) => ({ ...s, ...patch }));
}

function send(ws, frame) {
	if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(frame));
}

/**
 * Start viewing `host`: with a fresh `code` (from the link), else with the
 * session this browser stored for it. Paints the cached snapshot first.
 * @param {{ host: string, code?: string | null }} opts
 */
export async function startViewer({ host, code = null }) {
	installLifecycle();
	hostUrl = host;
	update({ state: 'connecting', host, reason: null });
	const cached = await snapshotGet(host);
	if (cached?.snapshot) {
		// §4.6 step 4: the last frame at once, marked stale until the host answers.
		viewerStats.cachedPaint = true;
		update({ stale: true });
		held = cached.snapshot;
		await render(cached.snapshot, { fetch: false });
	}
	if (code) {
		open({ host, code });
	} else {
		const stored = readStored(host);
		if (!stored?.session) {
			update({ state: 'failed', reason: 'no_session' });
			return;
		}
		open({ host, session: stored.session, have: { epoch: stored.epoch, revision: stored.revision } });
	}
}

/**
 * @param {{ host: string, code?: string, session?: string, have?: object }} opts
 */
function open({ host, code, session, have }) {
	if (socket) {
		socket.onclose = null;
		socket.close(1000);
		socket = null;
	}
	let ws;
	try {
		ws = new WebSocket(host);
	} catch (err) {
		update({ state: 'failed', reason: String(err?.message ?? err) });
		return;
	}
	ws.binaryType = 'arraybuffer';
	socket = ws;
	let byeReason = null;
	ws.onopen = () => {
		send(ws, {
			type: 'attach',
			protocol: VIEWER_PROTOCOL,
			...(code ? { code } : { session }),
			...(have?.epoch ? { have } : {}),
			// §4.5: the compact encoding when this browser can decode it.
			encodings: compactSupported() ? ['mq/1', 'raw/1'] : ['raw/1'],
			visible: typeof document === 'undefined' || document.visibilityState !== 'hidden'
		});
	};
	ws.onmessage = (event) => {
		if (event.data instanceof ArrayBuffer) {
			onBlob(event.data);
			return;
		}
		let frame;
		try {
			frame = JSON.parse(event.data);
		} catch {
			return;
		}
		switch (frame?.type) {
			case 'welcome': {
				reconnectDelayMs = RECONNECT_FIRST_MS;
				encoding = typeof frame.encoding === 'string' ? frame.encoding : 'raw/1';
				const stored = readStored(host);
				const sameViewer = stored?.session && held;
				writeStored(host, {
					session: frame.session,
					epoch: sameViewer ? stored.epoch : null,
					revision: sameViewer ? stored.revision : null
				});
				// §4.6 step 5: a host in the state this browser holds sends only
				// this; one that is not sends its snapshot right after.
				update({ state: 'attached', viewerId: frame.viewer_id, encoding, reason: null, stale: false });
				break;
			}
			case 'session':
				// A refreshed token (§4.7): the resume window runs from now.
				rememberSession(frame.session);
				break;
			case 'snapshot':
				viewerStats.snapshots += 1;
				held = frame;
				onState(frame);
				break;
			case 'update':
				viewerStats.updates += 1;
				// §4.3: latest-wins over the keys it carries. A viewer that
				// holds nothing yet cannot merge — ask for the whole thing.
				if (!held || held.epoch !== frame.epoch) {
					send(ws, { type: 'snapshot' });
					break;
				}
				held = { ...held, ...frame, type: 'snapshot' };
				onState(held);
				break;
			case 'rebuild':
				if (frame.state === 'started') viewerStats.rebuildsStarted += 1;
				if (frame.state === 'done') viewerStats.rebuildsDone += 1;
				if (typeof frame.tool === 'string') viewerStats.lastRebuildTool = frame.tool;
				update({
					rebuilding:
						frame.state === 'done'
							? null
							: {
									tool: frame.tool ?? '',
									feature: frame.feature_name ?? null,
									message: frame.message ?? null,
									elapsedMs: frame.elapsed_ms ?? 0
								}
				});
				break;
			case 'capture_request':
				answerCapture(ws, frame);
				break;
			case 'view_request':
				answerView(ws, frame);
				break;
			case 'ping':
				send(ws, { type: 'pong' });
				break;
			case 'pong':
				if (probeTimer) clearTimeout(probeTimer);
				probeTimer = null;
				break;
			case 'bye':
				byeReason = String(frame.reason ?? 'unknown');
				if (TERMINAL_BYE.has(byeReason)) writeStored(host, null);
				break;
			default:
				break;
		}
	};
	ws.onclose = () => {
		if (socket === ws) socket = null;
		for (const id of [...wanted.keys()]) settle(id, null);
		update({ rebuilding: null });
		if (byeReason !== null && TERMINAL_BYE.has(byeReason)) {
			update({ state: 'failed', reason: byeReason });
			return;
		}
		update({ state: 'reconnecting', reason: byeReason });
		scheduleReconnect();
	};
}

/** @param {string} session */
function rememberSession(session) {
	if (!hostUrl || typeof session !== 'string') return;
	const stored = readStored(hostUrl);
	writeStored(hostUrl, {
		session,
		epoch: stored?.epoch ?? null,
		revision: stored?.revision ?? null
	});
}

/** A snapshot or a merged update: draw it, remember it, cache it. */
async function onState(state) {
	if (!hostUrl) return;
	const stored = readStored(hostUrl);
	writeStored(hostUrl, {
		session: stored?.session ?? null,
		epoch: state.epoch ?? null,
		revision: state.revision ?? null
	});
	await render(state, { fetch: true });
	await snapshotPut(hostUrl, state, stored?.session ?? null);
}

/**
 * Draw `snapshot`: every body's blob from memory, else the cache, else (when
 * `fetch`) the host. A newer snapshot arriving meanwhile supersedes this one.
 * @param {any} snapshot
 * @param {{ fetch: boolean }} opts
 */
async function render(snapshot, { fetch }) {
	const serial = ++renderSerial;
	const bodies = Array.isArray(snapshot.bodies) ? snapshot.bodies : [];
	/** @type {string[]} */
	const missing = [];
	/** @type {Map<string, ArrayBuffer>} */
	const bytesById = new Map();
	for (const body of bodies) {
		const id = body.mesh_id;
		if (decoded.has(id) || memory.has(id)) {
			if (memory.has(id)) bytesById.set(id, memory.get(id));
			continue;
		}
		const cached = await blobGet(id);
		if (cached) {
			viewerStats.cacheHits += 1;
			memory.set(id, cached);
			bytesById.set(id, cached);
		} else {
			missing.push(id);
		}
	}
	if (missing.length && fetch && socket) {
		viewerStats.blobRequests += missing.length;
		const arrivals = missing.map((id) => want(id));
		send(socket, { type: 'want', mesh_ids: missing, encoding });
		const results = await Promise.all(arrivals);
		missing.forEach((id, i) => {
			if (results[i]) bytesById.set(id, results[i]);
		});
	}
	if (serial !== renderSerial) return; // superseded
	const meshes = [];
	for (const body of bodies) {
		const id = body.mesh_id;
		let arrays = decoded.get(id);
		if (!arrays) {
			const bytes = bytesById.get(id);
			if (!bytes) continue;
			try {
				arrays = await decodeArrays(bytes);
			} catch (err) {
				console.warn('viewer: undecodable blob', id, err);
				continue;
			}
			if (serial !== renderSerial) return; // superseded while decoding
			decoded.set(id, arrays);
			// The bytes are kept only until they are decoded; IndexedDB holds
			// the copy a reload paints from.
			memory.delete(id);
		}
		meshes.push(meshFromEntry(body, arrays));
	}
	forgetUnusedMeshes(bodies);
	applyViewerSnapshot(snapshot, meshes);
	update({
		epoch: snapshot.epoch ?? null,
		revision: snapshot.revision ?? null,
		documentName: snapshot.document?.name ?? null,
		bodies: meshes.length,
		agent: snapshot.activity?.agent ?? null,
		viewers: Array.isArray(snapshot.activity?.viewers) ? snapshot.activity.viewers.length : 0,
		stale: !fetch
	});
}

/** Decoded arrays for bodies no longer in the document are dead weight. */
function forgetUnusedMeshes(bodies) {
	const live = new Set(bodies.map((b) => b.mesh_id));
	for (const id of [...decoded.keys()]) {
		if (!live.has(id)) decoded.delete(id);
	}
	for (const id of [...memory.keys()]) {
		if (!live.has(id)) memory.delete(id);
	}
}

/** A promise for blob `id`, resolved by its frame (or null when missing / dropped). */
function want(id) {
	const existing = wanted.get(id);
	if (existing) return existing.promise;
	let resolve;
	const promise = new Promise((r) => {
		resolve = r;
	});
	wanted.set(id, { promise, resolve });
	return promise;
}

function settle(id, bytes) {
	const entry = wanted.get(id);
	if (!entry) return;
	wanted.delete(id);
	entry.resolve(bytes);
}

/** @param {ArrayBuffer} raw */
function onBlob(raw) {
	let frame;
	try {
		frame = decodeBlobFrame(raw);
	} catch {
		return;
	}
	const { header, payload } = frame;
	if (header?.type !== 'blob' || typeof header.mesh_id !== 'string') return;
	viewerStats.blobsReceived += 1;
	viewerStats.bytesReceived += payload.byteLength;
	memory.set(header.mesh_id, payload);
	settle(header.mesh_id, payload);
	blobPut(header.mesh_id, payload);
}

// -- the viewer's half of the viewport tools (§4.3) ----------------------------

/**
 * This viewer's selection, for `selection_get` over the link. Called by the
 * page whenever the store's selection changes; the payload is built by the
 * editor's own `selection_get`, so the answer has one implementation.
 * @param {Record<string, unknown>} payload
 */
export function sendSelection(payload) {
	if (!socket) return;
	send(socket, { type: 'select', ...payload });
}

async function answerCapture(ws, frame) {
	const { VIEWPORT_QUERIES } = await import('$lib/agent/viewport.js');
	try {
		const result = VIEWPORT_QUERIES.viewport_capture.run({ max_edge_px: frame.max_edge_px ?? 1024 });
		send(ws, {
			type: 'capture_result',
			id: frame.id,
			png_base64: result.content[0].data,
			width: result.structuredContent.width,
			height: result.structuredContent.height
		});
	} catch (err) {
		send(ws, { type: 'capture_result', id: frame.id, error: errorOf(err) });
	}
}

async function answerView(ws, frame) {
	const { VIEWPORT_QUERIES } = await import('$lib/agent/viewport.js');
	try {
		const result = VIEWPORT_QUERIES.viewport_view.run({ view: frame.view ?? null, fit: frame.fit !== false });
		send(ws, { type: 'view_result', id: frame.id, camera: result.structuredContent.camera });
	} catch (err) {
		send(ws, { type: 'view_result', id: frame.id, error: errorOf(err) });
	}
}

/** A `ToolFailure` (or anything thrown) as the frame's `error` block. */
function errorOf(err) {
	return {
		code: err?.code ?? 'ViewportUnavailable',
		message: err?.detail ?? err?.message ?? String(err),
		details: err?.details ?? {}
	};
}

// -- reconnect policy (the agent link's, §4.6) ---------------------------------

function clearTimers() {
	if (reconnectTimer) clearTimeout(reconnectTimer);
	if (probeTimer) clearTimeout(probeTimer);
	reconnectTimer = null;
	probeTimer = null;
}

function scheduleReconnect() {
	if (reconnectTimer) clearTimeout(reconnectTimer);
	reconnectTimer = null;
	if (!hostUrl || !readStored(hostUrl)?.session) {
		update({ state: 'failed', reason: 'no_session' });
		return;
	}
	if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;
	reconnectTimer = setTimeout(reconnectNow, reconnectDelayMs);
	reconnectDelayMs = Math.min(reconnectDelayMs * 2, RECONNECT_MAX_MS);
}

function reconnectNow() {
	reconnectTimer = null;
	if (socket || !hostUrl) return;
	const stored = readStored(hostUrl);
	if (!stored?.session) return;
	update({ state: 'reconnecting' });
	open({ host: hostUrl, session: stored.session, have: { epoch: stored.epoch, revision: stored.revision } });
}

function probeSocket() {
	const ws = socket;
	if (!ws || ws.readyState !== WebSocket.OPEN || probeTimer) return;
	send(ws, { type: 'ping' });
	probeTimer = setTimeout(() => {
		probeTimer = null;
		if (socket === ws) {
			ws.onclose = null;
			ws.close(4000, 'probe timeout');
			socket = null;
			reconnectDelayMs = RECONNECT_FIRST_MS;
			reconnectNow();
		}
	}, PROBE_TIMEOUT_MS);
}

function installLifecycle() {
	if (lifecycleInstalled || typeof window === 'undefined') return;
	lifecycleInstalled = true;
	const wake = () => {
		if (document.visibilityState !== 'visible') return;
		const { state } = get(viewerLink);
		if (state === 'reconnecting') {
			reconnectDelayMs = RECONNECT_FIRST_MS;
			if (reconnectTimer) clearTimeout(reconnectTimer);
			reconnectTimer = null;
			reconnectNow();
		} else if (state === 'attached') {
			probeSocket();
		}
	};
	document.addEventListener('visibilitychange', () => {
		if (socket) send(socket, { type: 'visible', visible: document.visibilityState === 'visible' });
		wake();
	});
	window.addEventListener('pageshow', wake);
	window.addEventListener('online', wake);
	// Test hook (viewer.spec.js).
	window.__waffleViewer = {
		state: () => get(viewerLink),
		stats: () => ({ ...viewerStats }),
		dropConnection: () => socket?.close(4001, 'test_drop')
	};
}

/** Detach for good (a settings action); the stored session is dropped. */
export function stopViewer() {
	clearTimers();
	const ws = socket;
	socket = null;
	if (hostUrl) writeStored(hostUrl, null);
	if (ws) {
		ws.onclose = null;
		send(ws, { type: 'bye', reason: 'user_left' });
		ws.close(1000, 'user_left');
	}
	update({ state: 'idle', reason: null });
}
