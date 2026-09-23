/**
 * The viewer's cache (specs/waffle_server_mode.md §4.4, §4.6): mesh blobs by
 * content id, and the last snapshot per host, in their own IndexedDB
 * database. A reloaded tab paints the last snapshot from here at once and
 * asks the host only for the blobs it does not hold; because the keys are
 * content hashes, a blob is valid across reconnects, host restarts and
 * documents.
 *
 * Bounded: blobs beyond `MAX_BLOB_BYTES` are dropped oldest-fetched first.
 * Every read and write is best effort — a private window or cleared site
 * data only costs a refetch.
 */

const DB_NAME = 'waffle-iron-viewer';
const DB_VERSION = 1;
const BLOBS = 'blobs';
const SNAPSHOTS = 'snapshots';
export const MAX_BLOB_BYTES = 256 * 1024 * 1024;

/** @type {Promise<IDBDatabase> | null} */
let dbPromise = null;

function openDB() {
	if (!dbPromise) {
		dbPromise = new Promise((resolve, reject) => {
			if (typeof indexedDB === 'undefined') {
				reject(new Error('IndexedDB unavailable'));
				return;
			}
			const req = indexedDB.open(DB_NAME, DB_VERSION);
			req.onupgradeneeded = () => {
				const db = req.result;
				if (!db.objectStoreNames.contains(BLOBS)) {
					db.createObjectStore(BLOBS, { keyPath: 'mesh_id' }).createIndex('fetched', 'fetched');
				}
				if (!db.objectStoreNames.contains(SNAPSHOTS)) {
					db.createObjectStore(SNAPSHOTS, { keyPath: 'host' });
				}
			};
			req.onsuccess = () => resolve(req.result);
			req.onerror = () => reject(req.error);
		});
		dbPromise.catch(() => {
			dbPromise = null;
		});
	}
	return dbPromise;
}

/**
 * @template T
 * @param {string} store
 * @param {IDBTransactionMode} mode
 * @param {(store: IDBObjectStore) => IDBRequest<T>} op
 * @returns {Promise<T>}
 */
async function run(store, mode, op) {
	const db = await openDB();
	return new Promise((resolve, reject) => {
		const req = op(db.transaction(store, mode).objectStore(store));
		req.onsuccess = () => resolve(req.result);
		req.onerror = () => reject(req.error);
	});
}

/** @param {string} meshId @returns {Promise<ArrayBuffer | null>} */
export async function blobGet(meshId) {
	try {
		const row = await run(BLOBS, 'readonly', (s) => s.get(meshId));
		return row?.bytes ?? null;
	} catch {
		return null;
	}
}

/** @param {string} meshId @param {ArrayBuffer} bytes */
export async function blobPut(meshId, bytes) {
	try {
		await run(BLOBS, 'readwrite', (s) => s.put({ mesh_id: meshId, bytes, fetched: Date.now() }));
		await evict();
	} catch {
		// a refetch later is the only consequence
	}
}

/** Drop the oldest-fetched blobs while the store is over its bound. */
async function evict() {
	const rows = /** @type {Array<{mesh_id: string, bytes: ArrayBuffer, fetched: number}>} */ (
		await run(BLOBS, 'readonly', (s) => s.getAll())
	);
	let total = rows.reduce((n, r) => n + r.bytes.byteLength, 0);
	if (total <= MAX_BLOB_BYTES) return;
	rows.sort((a, b) => a.fetched - b.fetched);
	for (const row of rows) {
		if (total <= MAX_BLOB_BYTES) break;
		await run(BLOBS, 'readwrite', (s) => s.delete(row.mesh_id));
		total -= row.bytes.byteLength;
	}
}

/**
 * The last snapshot this browser held for `host`, with the session that
 * resumes it.
 * @param {string} host
 * @returns {Promise<{ host: string, snapshot: any, session: string | null, saved: number } | null>}
 */
export async function snapshotGet(host) {
	try {
		return (await run(SNAPSHOTS, 'readonly', (s) => s.get(host))) ?? null;
	} catch {
		return null;
	}
}

/** @param {string} host @param {any} snapshot @param {string | null} session */
export async function snapshotPut(host, snapshot, session) {
	try {
		await run(SNAPSHOTS, 'readwrite', (s) => s.put({ host, snapshot, session, saved: Date.now() }));
	} catch {
		// the next attach fetches it
	}
}

/** Everything (a settings action, and the tests). */
export async function viewerCacheClear() {
	try {
		await run(BLOBS, 'readwrite', (s) => s.clear());
		await run(SNAPSHOTS, 'readwrite', (s) => s.clear());
	} catch {
		// nothing to clear
	}
}
