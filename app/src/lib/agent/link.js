/**
 * The page side of `waffle-agent-link/1` (specs/waffle_mcp_server.md §2.2, §2.3, §3.1).
 *
 * A MODULE-LEVEL singleton: the socket lives here, not in a component, so it
 * survives SvelteKit navigation from `/agent` to the editor.
 *
 * Consent (I7): a socket is opened only by `connectWithCode` (called from the
 * Allow click on `/agent`) or by `resumeAgentLink`, which needs a session token
 * that only a consented pairing in THIS tab can have written to sessionStorage.
 */
import { writable } from 'svelte/store';
import { getDocumentName } from '$lib/engine/store.svelte.js';
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

/**
 * @typedef {{
 *   state: 'idle' | 'connecting' | 'connected' | 'disconnected' | 'failed',
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

/** A failed pairing attempt: either the relay said `bye`, or the socket never opened. */
export class AgentLinkFailure extends Error {
	/**
	 * @param {'bye' | 'permission_denied' | 'security_error' | 'network_error'} kind
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

/** @param {{relay: string, session: string, agentName: string} | null} value */
function writeStored(value) {
	try {
		if (value) sessionStorage.setItem(STORAGE_KEY, JSON.stringify(value));
		else sessionStorage.removeItem(STORAGE_KEY);
	} catch {
		/* storage unavailable: the tab simply cannot resume after a reload */
	}
}

/**
 * Best-effort classification of a socket that never opened. Browsers expose no
 * reason on a failed WebSocket, so the only detectable class is an explicit
 * local-network permission denial (Chromium's Local Network Access).
 */
async function classifyFailure() {
	try {
		const status = await navigator.permissions.query(
			/** @type {PermissionDescriptor} */ ({ name: 'local-network-access' })
		);
		if (status.state === 'denied') return 'permission_denied';
	} catch {
		/* permission name unknown to this browser */
	}
	return 'network_error';
}

/** @param {object} frame */
function send(ws, frame) {
	if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(frame));
}

/** @param {WebSocket} ws @param {any} frame */
async function handleCall(ws, frame) {
	let result;
	try {
		result = await executeCall(frame);
	} catch (err) {
		result = { type: 'result', id: frame.id, ...toolError('Internal', String(err?.message ?? err)) };
	}
	send(ws, result);
}

/**
 * Open the socket and say hello. Resolves with the `welcome` frame.
 * @param {{ relay: string, code?: string, session?: string, agentName: string }} opts
 * @returns {Promise<any>}
 */
function open({ relay, code, session, agentName }) {
	if (socket) {
		socket.onclose = null;
		socket.close(1000);
		socket = null;
	}
	agentLink.set({ state: 'connecting', agentName, relay, reason: null, errorClass: null });

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
					const name = frame.agent_name || agentName;
					writeStored({ relay, session: frame.session, agentName: name });
					agentLink.set({ state: 'connected', agentName: name, relay, reason: null, errorClass: null });
					if (frame.manifest_required) send(ws, { type: 'manifest', tools: TOOLS.map(toManifestTool) });
					send(ws, { type: 'status', state: 'ready', document_name: getDocumentName() });
					resolve(frame);
					break;
				}
				case 'ping':
					send(ws, { type: 'pong' });
					break;
				case 'call':
					if (welcomed) handleCall(ws, frame);
					break;
				case 'bye':
					byeReason = String(frame.reason ?? 'unknown');
					if (TERMINAL_BYE.has(byeReason)) writeStored(null);
					break;
				default:
					break; // `cancel`: Phase 0 page tools are synchronous queries.
			}
		};

		ws.onclose = async () => {
			if (socket === ws) socket = null;
			if (!welcomed) {
				if (byeReason) {
					agentLink.set({ state: 'failed', agentName, relay, reason: byeReason, errorClass: 'bye' });
					reject(new AgentLinkFailure('bye', `relay refused the link: ${byeReason}`, byeReason));
				} else {
					const errorClass = await classifyFailure();
					agentLink.set({ state: 'failed', agentName, relay, reason: null, errorClass });
					reject(new AgentLinkFailure(/** @type {any} */ (errorClass), 'the WebSocket did not open'));
				}
				return;
			}
			agentLink.update((s) => ({ ...s, state: 'disconnected', reason: byeReason ?? 'connection_lost' }));
		};
	});
}

/**
 * Pair with a relay using a single-use code. Call ONLY from a user click (I7).
 * @param {{ relay: string, code: string, agentName: string }} opts
 */
export function connectWithCode({ relay, code, agentName }) {
	return open({ relay, code, agentName });
}

/**
 * Same-tab resume after a reload (P7): reconnects only if this tab holds a
 * session from an earlier consented pairing. No-op otherwise.
 */
export function resumeAgentLink() {
	if (socket) return;
	const stored = readStored();
	if (!stored?.relay || !stored?.session) return;
	open({ relay: stored.relay, session: stored.session, agentName: stored.agentName }).catch(() => {
		// The failure is already reflected in `agentLink` (and a terminal bye cleared storage).
	});
}

/** The user's Disconnect (P12): revokes the session; no resume. */
export function disconnectAgentLink() {
	const ws = socket;
	writeStored(null);
	socket = null;
	if (ws) {
		ws.onclose = null;
		send(ws, { type: 'bye', reason: 'user_disconnected' });
		ws.close(1000, 'user_disconnected');
	}
	agentLink.update((s) => ({ ...s, state: 'idle', reason: 'user_disconnected' }));
}
