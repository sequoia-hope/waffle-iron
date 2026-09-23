/**
 * The page side of `waffle-agent-link/1` (specs/waffle_mcp_server.md §2.2, §2.3, §3.1).
 *
 * A MODULE-LEVEL singleton: the socket lives here, not in a component, so it
 * survives SvelteKit navigation from `/agent` to the editor.
 *
 * Consent (I7): a socket is opened only by `connectWithCode` (called from the
 * Allow click on `/agent`) or by a resume, which needs a session token that
 * only a consented pairing in THIS tab can have written to sessionStorage.
 *
 * Reconnect (§2.2, P7): a lost session is resumed by the page itself — at once
 * when the tab becomes visible or the network returns, else with backoff while
 * visible. A mobile browser suspends a background tab, so its socket dies
 * while the user is in another app; nothing retries while hidden.
 */
import { get, writable } from 'svelte/store';
import {
	getDocumentInfo,
	getDocumentName,
	reopenDocumentById,
	subscribeRebuildProgress,
	whenStartupRestoreSettled
} from '$lib/engine/store.svelte.js';
import { showToast } from '$lib/ui/toast.svelte.js';
import { executeCall, toolError } from './executor.js';
import { TOOLS } from './tools/index.js';
import { LINK_PROTOCOL, canonicalJson, toManifestTool, webSha256Hex } from './tools/manifest.js';

const STORAGE_KEY = 'waffle-agent-link';

/** `bye` reasons after which the stored session is useless. */
const TERMINAL_BYE = new Set([
	'invalid_code',
	'already_paired',
	'session_expired',
	'revoked',
	'user_disconnected',
	'protocol_mismatch'
]);

const RECONNECT_FIRST_MS = 1000;
const RECONNECT_MAX_MS = 30000;
/** A visible tab's socket that looks open must answer a `ping` within this. */
const PROBE_TIMEOUT_MS = 5000;

/**
 * @typedef {{
 *   state: 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'disconnected' | 'failed',
 *   agentName: string | null,
 *   relay: string | null,
 *   reason: string | null,
 *   errorClass: string | null,
 * }} AgentLinkStatus
 */

/** @type {import('svelte/store').Writable<AgentLinkStatus>} */
export const agentLink = writable({
	state: 'idle',
	agentName: null,
	relay: null,
	reason: null,
	errorClass: null
});

/**
 * Pause state (§1, G4, I12). `reason` is set when the page paused the session
 * itself (a broken invariant or an engine crash), null for the user's Pause.
 * @type {import('svelte/store').Writable<{ paused: boolean, reason: string | null }>}
 */
export const agentPause = writable({ paused: false, reason: null });

/** A failed pairing attempt: either the relay said `bye`, or the socket never opened. */
export class AgentLinkFailure extends Error {
	/**
	 * @param {'bye' | 'permission_denied' | 'permission_blocked' | 'security_error' | 'network_error'} kind
	 * @param {string} message
	 * @param {string | null} [reason]
	 */
	constructor(kind, message, reason = null) {
		super(message);
		this.kind = kind;
		this.reason = reason;
	}
}

/** @type {WebSocket | null} */
let socket = null;

/** @type {Promise<string | null> | null} */
let manifestHashPromise = null;

/** Ids of calls the relay cancelled (A18). */
const cancelledCalls = new Set();

/** The call being executed (the page runs one at a time), for progress. */
/** @type {string | null} */
let activeCallId = null;
let activeCallStartedAt = 0;

// Rebuild progress of the call in flight goes to the relay as `progress`
// frames (`specs/waffle_mcp_server.md` §2.3; `specs/b4_balanced_union.md`
// §2.3). A frame that arrives between calls belongs to the user's own
// action and stays on the page.
subscribeRebuildProgress((msg) => {
	const ws = socket;
	if (!ws || ws.readyState !== WebSocket.OPEN || activeCallId === null) return;
	send(ws, {
		type: 'progress',
		id: activeCallId,
		message: `${msg.feature_name}: ${msg.label}`,
		elapsed_ms: Math.round(performance.now() - activeCallStartedAt),
		progress: msg.done,
		total: msg.done + msg.remaining
	});
});

/** Last status frame sent on the current socket, to send only changes. */
let lastStatusKey = '';

/** @type {ReturnType<typeof setTimeout> | null} */
let reconnectTimer = null;
let reconnectDelayMs = RECONNECT_FIRST_MS;
/** @type {ReturnType<typeof setTimeout> | null} */
let probeTimer = null;
let lifecycleInstalled = false;
/** The page-load resume ran (it is the only one that says `reloaded`). */
let loadResumeStarted = false;

function manifestHash() {
	if (!manifestHashPromise) {
		manifestHashPromise =
			typeof crypto !== 'undefined' && crypto.subtle
				? webSha256Hex(canonicalJson(TOOLS.map(toManifestTool))).catch(() => null)
				: Promise.resolve(null);
	}
	return manifestHashPromise;
}

function readStored() {
	try {
		const raw = sessionStorage.getItem(STORAGE_KEY);
		return raw ? JSON.parse(raw) : null;
	} catch {
		return null;
	}
}

/** @param {{relay: string, session: string, agentName: string, docId?: string | null} | null} value */
function writeStored(value) {
	try {
		if (value) sessionStorage.setItem(STORAGE_KEY, JSON.stringify(value));
		else sessionStorage.removeItem(STORAGE_KEY);
	} catch {
		/* storage unavailable: the tab simply cannot resume after a reload */
	}
}

/**
 * Remember, with the session, which storage record the agent is working on,
 * so a reloaded tab can land on it again (`resumeAgentLink`) whatever its
 * restore policy does with the blank startup document. Written when it
 * changes: after every call and on every status frame.
 */
function noteAgentDocument() {
	const stored = readStored();
	if (!stored?.session) return;
	const docId = getDocumentInfo().storageId ?? null;
	if (stored.docId === docId) return;
	writeStored({ ...stored, docId });
}

/**
 * Best-effort classification of a socket that never opened. Browsers expose no
 * reason on a failed WebSocket; Chromium's Local Network Access permission is
 * the one class the page can read (spec §6.3: `navigator.permissions.query`
 * answers `prompt` / `granted` / `denied` for `local-network-access`).
 * @param {string} relay
 */
async function classifyFailure(relay) {
	try {
		const status = await navigator.permissions.query(
			/** @type {PermissionDescriptor} */ ({ name: 'local-network-access' })
		);
		if (status.state === 'denied') return 'permission_denied';
		// Not yet granted: from a public origin the browser blocks the loopback
		// socket until the user allows local network access (a headless or
		// dismissed prompt leaves the state at `prompt`).
		if (status.state === 'prompt' && isPublicPage() && isLocalRelay(relay)) return 'permission_blocked';
	} catch {
		/* permission name unknown to this browser */
	}
	return 'network_error';
}

function isPublicPage() {
	const host = location.hostname;
	return !(host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || host.endsWith('.localhost'));
}

/** @param {string} relay */
function isLocalRelay(relay) {
	try {
		const host = new URL(relay).hostname;
		return host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || /^(10|192\.168|172\.(1[6-9]|2\d|3[01]))\./.test(host);
	} catch {
		return false;
	}
}

/** @param {WebSocket} ws @param {object} frame */
function send(ws, frame) {
	if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(frame));
}

/**
 * Pause (or resume) the agent (§1, G4). A pause stops the NEXT command from
 * being admitted; a running call completes or rolls back first (I12).
 * @param {boolean} paused
 * @param {string | null} [reason] - why the page paused the session itself
 */
export function setAgentPaused(paused, reason = null) {
	agentPause.set({ paused, reason: paused ? reason : null });
}

/**
 * Report the page state to the relay (`status` frame, §2.3) when it changed.
 * @param {{ state: 'ready' | 'paused' | 'busy', reason?: string | null, document_name?: string }} status
 */
export function sendAgentStatus(status) {
	const ws = socket;
	if (!ws || ws.readyState !== WebSocket.OPEN) return;
	const frame = { type: 'status', state: status.state, document_name: status.document_name ?? getDocumentName() };
	if (status.state === 'busy' && status.reason) frame.reason = status.reason;
	noteAgentDocument();
	const key = JSON.stringify(frame);
	if (key === lastStatusKey) return;
	lastStatusKey = key;
	send(ws, frame);
}

/** @param {WebSocket} ws @param {any} frame */
async function handleCall(ws, frame) {
	const ctx = {
		agentName: get(agentLink).agentName ?? 'agent',
		isPaused: () => get(agentPause).paused,
		pause: (/** @type {string} */ reason) => setAgentPaused(true, reason),
		isCancelled: () => cancelledCalls.has(frame.id)
	};
	let result;
	activeCallId = String(frame.id);
	activeCallStartedAt = performance.now();
	try {
		result = await executeCall(frame, ctx);
	} catch (err) {
		result = { type: 'result', id: frame.id, ...toolError('Internal', String(err?.message ?? err)) };
	} finally {
		cancelledCalls.delete(frame.id);
		if (activeCallId === String(frame.id)) activeCallId = null;
	}
	noteAgentDocument();
	send(ws, result);
}

function clearTimers() {
	if (reconnectTimer) clearTimeout(reconnectTimer);
	if (probeTimer) clearTimeout(probeTimer);
	reconnectTimer = null;
	probeTimer = null;
}

/**
 * Open the socket and say hello. Resolves with the `welcome` frame.
 * `phase: 'reconnecting'` resumes a stored session: a failure that is not a
 * terminal `bye` keeps retrying instead of failing.
 * @param {{ relay: string, code?: string, session?: string, agentName: string, reloaded?: boolean, phase?: 'connecting' | 'reconnecting' }} opts
 * @returns {Promise<any>}
 */
function open({ relay, code, session, agentName, reloaded = false, phase = 'connecting' }) {
	if (socket) {
		socket.onclose = null;
		socket.close(1000);
		socket = null;
	}
	agentLink.set({ state: phase, agentName, relay, reason: null, errorClass: null });

	return new Promise((resolve, reject) => {
		/** @type {WebSocket} */
		let ws;
		try {
			ws = new WebSocket(relay);
		} catch (err) {
			agentLink.set({ state: 'failed', agentName, relay, reason: null, errorClass: 'security_error' });
			reject(new AgentLinkFailure('security_error', String(err?.message ?? err)));
			return;
		}
		socket = ws;
		let welcomed = false;
		/** @type {string | null} */
		let byeReason = null;

		ws.onopen = async () => {
			const hash = await manifestHash();
			send(ws, {
				type: 'hello',
				protocol: LINK_PROTOCOL,
				...(code ? { code } : { session }),
				...(reloaded ? { reloaded: true } : {}),
				app_build: typeof __BUILD_INFO__ !== 'undefined' ? __BUILD_INFO__ : null,
				manifest_hash: hash
			});
		};

		ws.onmessage = (event) => {
			let frame;
			try {
				frame = JSON.parse(event.data);
			} catch {
				return;
			}
			switch (frame?.type) {
				case 'welcome': {
					welcomed = true;
					reconnectDelayMs = RECONNECT_FIRST_MS;
					const name = frame.agent_name || agentName;
					writeStored({ relay, session: frame.session, agentName: name, docId: getDocumentInfo().storageId ?? null });
					agentLink.set({ state: 'connected', agentName: name, relay, reason: null, errorClass: null });
					if (frame.manifest_required) send(ws, { type: 'manifest', tools: TOOLS.map(toManifestTool) });
					lastStatusKey = '';
					const { paused } = get(agentPause);
					sendAgentStatus({ state: paused ? 'paused' : 'ready' });
					resolve(frame);
					break;
				}
				case 'ping':
					send(ws, { type: 'pong' });
					break;
				case 'pong':
					if (probeTimer) clearTimeout(probeTimer);
					probeTimer = null;
					break;
				case 'call':
					if (welcomed) handleCall(ws, frame);
					break;
				case 'cancel':
					cancelledCalls.add(String(frame.id));
					break;
				case 'bye':
					byeReason = String(frame.reason ?? 'unknown');
					if (TERMINAL_BYE.has(byeReason)) writeStored(null);
					break;
				default:
					break;
			}
		};

		ws.onclose = async () => {
			if (socket === ws) socket = null;
			const terminal = byeReason !== null && TERMINAL_BYE.has(byeReason);
			if (!welcomed) {
				if (phase === 'reconnecting' && !terminal) {
					// The relay is unreachable for now (network, relay restarting): keep trying.
					agentLink.update((s) => ({ ...s, state: 'reconnecting', reason: byeReason ?? 'connection_lost' }));
					scheduleReconnect();
					reject(new AgentLinkFailure('network_error', 'the WebSocket did not open'));
				} else if (phase === 'reconnecting') {
					agentLink.set({ state: 'disconnected', agentName, relay, reason: byeReason, errorClass: 'bye' });
					reject(new AgentLinkFailure('bye', `relay refused the session: ${byeReason}`, byeReason));
				} else if (byeReason) {
					agentLink.set({ state: 'failed', agentName, relay, reason: byeReason, errorClass: 'bye' });
					reject(new AgentLinkFailure('bye', `relay refused the link: ${byeReason}`, byeReason));
				} else {
					const errorClass = await classifyFailure(relay);
					agentLink.set({ state: 'failed', agentName, relay, reason: null, errorClass });
					reject(new AgentLinkFailure(/** @type {any} */ (errorClass), 'the WebSocket did not open'));
				}
				return;
			}
			if (terminal) {
				agentLink.update((s) => ({ ...s, state: 'disconnected', reason: byeReason }));
				return;
			}
			agentLink.update((s) => ({ ...s, state: 'reconnecting', reason: byeReason ?? 'connection_lost' }));
			scheduleReconnect();
		};
	});
}

/** Retry the stored session after the current backoff, if the tab is visible. */
function scheduleReconnect() {
	if (reconnectTimer) clearTimeout(reconnectTimer);
	reconnectTimer = null;
	if (!readStored()?.session) {
		agentLink.update((s) => ({ ...s, state: 'disconnected' }));
		return;
	}
	// A hidden tab is suspended or about to be; `visibilitychange` retries it.
	if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return;
	const delay = reconnectDelayMs;
	reconnectDelayMs = Math.min(reconnectDelayMs * 2, RECONNECT_MAX_MS);
	reconnectTimer = setTimeout(() => {
		reconnectTimer = null;
		reconnectNow();
	}, delay);
}

function reconnectNow() {
	if (socket) return;
	const stored = readStored();
	if (!stored?.relay || !stored?.session) return;
	if (reconnectTimer) clearTimeout(reconnectTimer);
	reconnectTimer = null;
	open({ relay: stored.relay, session: stored.session, agentName: stored.agentName, phase: 'reconnecting' }).catch(() => {
		// Reflected in `agentLink`; a non-terminal failure already scheduled the next try.
	});
}

/**
 * A socket that looks open after the tab was frozen may be long dead (the
 * relay dropped it on heartbeat, the TCP close never arrived): ask for a pong.
 */
function probeSocket() {
	const ws = socket;
	if (!ws || ws.readyState !== WebSocket.OPEN || probeTimer) return;
	send(ws, { type: 'ping' });
	probeTimer = setTimeout(() => {
		probeTimer = null;
		if (socket === ws) dropSocket(ws, 'probe_timeout');
	}, PROBE_TIMEOUT_MS);
}

/**
 * Treat `ws` as lost now and resume at once; its close event may never come.
 * @param {WebSocket} ws @param {string} reason
 */
function dropSocket(ws, reason) {
	ws.onclose = null;
	ws.onmessage = null;
	try {
		ws.close(4000, reason);
	} catch {
		/* already closed */
	}
	if (socket === ws) socket = null;
	agentLink.update((s) => ({ ...s, state: 'reconnecting', reason: 'connection_lost' }));
	reconnectDelayMs = RECONNECT_FIRST_MS;
	reconnectNow();
}

function installLifecycle() {
	if (lifecycleInstalled || typeof document === 'undefined') return;
	lifecycleInstalled = true;
	const wake = () => {
		if (document.visibilityState !== 'visible') return;
		const { state } = get(agentLink);
		if (state === 'reconnecting') {
			reconnectDelayMs = RECONNECT_FIRST_MS;
			reconnectNow();
		} else if (state === 'connected') {
			probeSocket();
		}
	};
	document.addEventListener('visibilitychange', wake);
	window.addEventListener('pageshow', wake);
	window.addEventListener('online', wake);
	// Test hook (agent-reconnect.spec.js): close the socket as a network drop would.
	window.__waffleAgentLink = {
		dropConnection: () => socket?.close(4001, 'test_drop')
	};
}

/**
 * Pair with a relay using a single-use code. Call ONLY from a user click (I7).
 * @param {{ relay: string, code: string, agentName: string }} opts
 */
export function connectWithCode({ relay, code, agentName }) {
	installLifecycle();
	clearTimers();
	reconnectDelayMs = RECONNECT_FIRST_MS;
	setAgentPaused(false);
	return open({ relay, code, agentName });
}

/**
 * Resume after a page load (P7): reconnects only if this tab holds a session
 * from an earlier consented pairing, once the tab's startup restore settled,
 * so the agent never lands on the blank bootstrap document of a tab that is
 * reopening its work. No-op otherwise, and after the first call.
 */
export async function resumeAgentLink() {
	installLifecycle();
	if (socket || loadResumeStarted) return;
	const stored = readStored();
	if (!stored?.relay || !stored?.session) return;
	loadResumeStarted = true;
	agentLink.set({ state: 'reconnecting', agentName: stored.agentName, relay: stored.relay, reason: null, errorClass: null });
	await whenStartupRestoreSettled();
	if (socket || !readStored()?.session) return;
	// The agent's document, whatever the restore policy did: a `never` (or a
	// discarded offer) leaves the blank startup document, and an agent mid-work
	// then builds on nothing — what happened on 2026-09-23 when iOS reloaded
	// the tab between two assembly calls.
	if (stored.docId && getDocumentInfo().storageId !== stored.docId) {
		try {
			if (await reopenDocumentById(stored.docId)) {
				showToast('info', `Reopened the agent's document: ${getDocumentName()}`);
			}
		} catch (err) {
			// The relay's reload note tells the agent to check; the page can only try.
			console.warn('Reopening the agent document failed:', err?.message ?? err);
		}
	}
	open({ relay: stored.relay, session: stored.session, agentName: stored.agentName, reloaded: true, phase: 'reconnecting' }).catch(() => {
		// The failure is already reflected in `agentLink` (and a terminal bye cleared storage).
	});
}

/** The user's Disconnect (P12): revokes the session; no resume. */
export function disconnectAgentLink() {
	const ws = socket;
	writeStored(null);
	clearTimers();
	socket = null;
	if (ws) {
		ws.onclose = null;
		send(ws, { type: 'bye', reason: 'user_disconnected' });
		ws.close(1000, 'user_disconnected');
	}
	setAgentPaused(false);
	agentLink.update((s) => ({ ...s, state: 'idle', reason: 'user_disconnected' }));
}
