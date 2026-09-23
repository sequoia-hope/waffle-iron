/**
 * Per-tab drafts: the open document's latest composed `.waffle` text, kept in
 * this browser whatever the active storage provider is, so work survives a
 * reload or the OS killing the tab (iOS discards backgrounded tabs).
 *
 * One record per browser tab, keyed by a tab key held in `sessionStorage`
 * (which a reloaded or OS-restored tab keeps). A draft is a COPY: deleting one
 * never touches the stored document it came from.
 */

const DB_NAME = 'waffle-iron-drafts';
const DB_VERSION = 1;
const STORE_NAME = 'drafts';
const TAB_KEY = 'waffle-tab-key';
/**
 * The key of the tab that was hidden last, in `localStorage`. An iOS tab
 * discard does not reliably keep `sessionStorage` (specs/waffle_server_mode.md
 * §4.6): a reloaded tab that finds no key of its own adopts this one, once,
 * and so still finds its draft. Another tab of the same browser could adopt
 * it instead — drafts are copies, so that costs nothing but a restore offer.
 */
const LAST_HIDDEN_TAB_KEY = 'waffle-tab-key-last-hidden';
/** A hidden tab's key is adopted only this long after it was hidden. */
const LAST_HIDDEN_MAX_AGE_MS = 24 * 60 * 60 * 1000;

/** Drafts untouched this long are dropped at startup. */
export const DRAFT_MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;

/** This tab's key once minted or adopted, so a later `sessionStorage` loss cannot change it. */
/** @type {string | null} */
let cachedTabKey = null;

/**
 * `sketch` is the open sketch session, which the document does not hold until
 * the sketch is finished (store.svelte.js `sketchSessionSnapshot`); null when
 * no sketch is open.
 * @typedef {{ tabKey: string, docId: string, name: string, json: string, sketch: object | null, modified: number }} Draft
 */

/** @type {Promise<IDBDatabase> | null} */
let dbPromise = null;

function openDB() {
	if (!dbPromise) {
		dbPromise = new Promise((resolve, reject) => {
			const request = indexedDB.open(DB_NAME, DB_VERSION);
			request.onupgradeneeded = () => {
				const db = request.result;
				if (!db.objectStoreNames.contains(STORE_NAME)) {
					db.createObjectStore(STORE_NAME, { keyPath: 'tabKey' }).createIndex('modified', 'modified');
				}
			};
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => reject(request.error);
		});
		dbPromise.catch(() => {
			dbPromise = null;
		});
	}
	return dbPromise;
}

/**
 * @template T
 * @param {IDBTransactionMode} mode
 * @param {(store: IDBObjectStore) => IDBRequest<T>} op
 * @returns {Promise<T>}
 */
async function run(mode, op) {
	const db = await openDB();
	return new Promise((resolve, reject) => {
		const request = op(db.transaction(STORE_NAME, mode).objectStore(STORE_NAME));
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});
}

/**
 * This browser tab's key, minted on first use. Null when sessionStorage is
 * unavailable (the tab then has no draft of its own).
 * @returns {string | null}
 */
export function tabKey() {
	if (cachedTabKey) return cachedTabKey;
	try {
		let key = sessionStorage.getItem(TAB_KEY);
		if (!key) key = adoptLastHiddenTabKey();
		if (!key) {
			key = typeof crypto !== 'undefined' && crypto.randomUUID
				? crypto.randomUUID()
				: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
		}
		sessionStorage.setItem(TAB_KEY, key);
		cachedTabKey = key;
		return key;
	} catch {
		return null;
	}
}

/** The last hidden tab's key, taken (single use) when it is recent; else null. */
function adoptLastHiddenTabKey() {
	try {
		const raw = localStorage.getItem(LAST_HIDDEN_TAB_KEY);
		if (!raw) return null;
		localStorage.removeItem(LAST_HIDDEN_TAB_KEY);
		const { key, at } = JSON.parse(raw);
		if (typeof key !== 'string' || typeof at !== 'number') return null;
		return Date.now() - at <= LAST_HIDDEN_MAX_AGE_MS ? key : null;
	} catch {
		return null;
	}
}

/**
 * Record this tab's key as the last hidden one. Called when the tab is hidden
 * or unloaded — the moment before the OS may discard it.
 */
export function rememberTabKey() {
	const key = tabKey();
	if (!key) return;
	try {
		localStorage.setItem(LAST_HIDDEN_TAB_KEY, JSON.stringify({ key, at: Date.now() }));
	} catch {
		/* storage unavailable */
	}
}

/**
 * Write this tab's draft.
 * @param {{ docId: string, name: string, json: string, sketch?: object | null }} draft
 */
export async function putDraft({ docId, name, json, sketch = null }) {
	const key = tabKey();
	if (!key) return;
	/** @type {Draft} */
	const record = { tabKey: key, docId, name, json, sketch, modified: Date.now() };
	await run('readwrite', (s) => s.put(record));
}

/**
 * @param {string} key
 * @returns {Promise<Draft | null>}
 */
export async function getDraft(key) {
	return (await run('readonly', (s) => s.get(key))) ?? null;
}

/** @returns {Promise<Draft[]>} every draft, newest first */
export async function listDrafts() {
	const all = /** @type {Draft[]} */ (await run('readonly', (s) => s.getAll()));
	return all.sort((a, b) => b.modified - a.modified);
}

/** @param {string} key */
export async function deleteDraft(key) {
	await run('readwrite', (s) => s.delete(key));
}

/** Drop drafts older than `maxAgeMs`. */
export async function pruneDrafts(maxAgeMs = DRAFT_MAX_AGE_MS) {
	const cutoff = Date.now() - maxAgeMs;
	for (const d of await listDrafts()) {
		if (d.modified < cutoff) await deleteDraft(d.tabKey);
	}
}
