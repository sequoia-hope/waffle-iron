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

/** Drafts untouched this long are dropped at startup. */
export const DRAFT_MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;

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
	try {
		let key = sessionStorage.getItem(TAB_KEY);
		if (!key) {
			key = typeof crypto !== 'undefined' && crypto.randomUUID
				? crypto.randomUUID()
				: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
			sessionStorage.setItem(TAB_KEY, key);
		}
		return key;
	} catch {
		return null;
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
