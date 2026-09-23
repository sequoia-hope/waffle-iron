/**
 * The viewer side of `waffle-viewer/1` (specs/waffle_server_mode.md §4): a
 * `/view` page attaches to a relay running `--kernel host`, holds no
 * authoritative state, and draws what the host computes.
 *
 * - `attach{code}` from the link `waffle_connect` handed out, or
 *   `attach{session, have}` on a reload / reconnect. `have` names the state
 *   this browser already holds; a host in the same state sends only `welcome`.
 * - Every `snapshot` is the whole document minus the geometry; the bodies it
 *   names are fetched by content id (`want` → binary `blob` frames), from
 *   the cache first.
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
import { decodeBlobFrame, decodeMesh } from './decode.js';

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
	reason: null
});

/** Counters the tests read: what was fetched, what the cache answered. */
export const viewerStats = { snapshots: 0, blobRequests: 0, blobsReceived: 0, cacheHits: 0, cachedPaint: false };

/** @type {WebSocket | null} */
let socket = null;
/** @type {string | null} */
let hostUrl = null;
/** The blobs decoded this session, by id: the memory tier of the cache. */
const memory = new Map();
/** Blob requests in flight, by id. */
const wanted = new Map();
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

/** @param {string} host @param {{ session: string, epoch: string | null, revision: number | null } | null} value */
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
				const stored = readStored(host);
				writeStored(host, {
					session: frame.session,
					epoch: stored?.session === frame.session ? stored.epoch : null,
					revision: stored?.session === frame.session ? stored.revision : null
				});
				// §4.6 step 5: a host in the state this browser holds sends only
				// this; one that is not sends its snapshot right after.
				update({ state: 'attached', viewerId: frame.viewer_id, reason: null, stale: false });
				break;
			}
			case 'snapshot':
				viewerStats.snapshots += 1;
				onSnapshot(frame);
				break;
			case 'blob':
				// A text `blob` frame is a miss: the host no longer has that id.
				if (frame.missing) settle(frame.mesh_id, null);
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
		if (byeReason !== null && TERMINAL_BYE.has(byeReason)) {
			update({ state: 'failed', reason: byeReason });
			return;
		}
		update({ state: 'reconnecting', reason: byeReason });
		scheduleReconnect();
	};
}

/** @param {any} snapshot */
async function onSnapshot(snapshot) {
	if (!hostUrl) return;
	const stored = readStored(hostUrl);
	writeStored(hostUrl, {
		session: stored?.session ?? null,
		epoch: snapshot.epoch ?? null,
		revision: snapshot.revision ?? null
	});
	await render(snapshot, { fetch: true });
	await snapshotPut(hostUrl, snapshot, stored?.session ?? null);
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
		if (memory.has(id)) {
			bytesById.set(id, memory.get(id));
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
		send(socket, { type: 'want', mesh_ids: missing });
		const results = await Promise.all(arrivals);
		missing.forEach((id, i) => {
			if (results[i]) bytesById.set(id, results[i]);
		});
	}
	if (serial !== renderSerial) return; // superseded
	const meshes = [];
	for (const body of bodies) {
		const bytes = bytesById.get(body.mesh_id);
		if (!bytes) continue;
		try {
			meshes.push(decodeMesh(body, bytes));
		} catch (err) {
			console.warn('viewer: undecodable blob', body.mesh_id, err);
		}
	}
	applyViewerSnapshot(snapshot, meshes);
	update({
		epoch: snapshot.epoch ?? null,
		revision: snapshot.revision ?? null,
		documentName: snapshot.document?.name ?? null,
		bodies: meshes.length,
		stale: !fetch
	});
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
	let decoded;
	try {
		decoded = decodeBlobFrame(raw);
	} catch {
		return;
	}
	const { header, payload } = decoded;
	if (header?.type !== 'blob' || typeof header.mesh_id !== 'string') return;
	viewerStats.blobsReceived += 1;
	memory.set(header.mesh_id, payload);
	settle(header.mesh_id, payload);
	blobPut(header.mesh_id, payload);
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
