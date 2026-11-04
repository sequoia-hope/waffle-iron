/**
 * Browser-local `DocumentStore` backed by IndexedDB (`waffle-iron` / `documents`).
 *
 * ## Revision hash choice (`git-blob-sha1:<40 hex>`)
 *
 * Each record's `rev` is `gitBlobSha1(doc.json)` (`./git/hash.js`): WebCrypto
 * SHA-1 over `"blob " + byteLength + "\0" + utf8Bytes` of the stored `.waffle`
 * JSON string. We use Git's blob SHA-1 because:
 *   1. It is already the repository-wide content hash (`SourceEntry.content_hash`,
 *      `open-link.js`, `crates/file-format`), implemented with zero dependencies
 *      on standard `crypto.subtle.digest('SHA-1', ...)`.
 *   2. It hashes `doc.json` directly rather than wrapper fields like `modified`,
 *      and matches the blob SHAs reported natively by the Git storage providers.
 *   3. Pre-existing or test-seeded IndexedDB records that carry no `.rev` field
 *      derive the exact same deterministic revision on read and compare-and-swap.
 */

import { gitBlobSha1 } from './git/hash.js';
import { generateDocId } from './types.js';

const DB_NAME = 'waffle-iron';
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const DB_VERSION = 1;
const STORE_NAME = 'documents';
const WATCH_CHANNEL = 'waffle-iron-storage';

/**
 * Open (or create) the IndexedDB database.
 * @returns {Promise<IDBDatabase>}
 */
function openDB() {
	return new Promise((resolve, reject) => {
		const request = indexedDB.open(DB_NAME, DB_VERSION);
		request.onupgradeneeded = (event) => {
			const db = event.target.result;
			if (!db.objectStoreNames.contains(STORE_NAME)) {
				const store = db.createObjectStore(STORE_NAME, { keyPath: 'id' });
				store.createIndex('modified', 'modified', { unique: false });
			}
		};
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});
}

/**
 * Project a raw IndexedDB record onto the public `StoredDocument` shape.
 * @param {any} raw
 * @returns {import('./types.js').StoredDocument}
 */
function toStoredDocument(raw) {
	return {
		id: raw.id,
		json: raw.json,
		created: raw.created,
		modified: raw.modified,
		...(raw.link !== undefined ? { link: raw.link } : {}),
		...(raw.localFallback ? { localFallback: true } : {})
	};
}

/**
 * Validate `put(doc, options)` options so missing `base` fails loudly instead
 * of bypassing concurrency checks.
 * @param {import('./types.js').DocumentPutOptions} options
 * @returns {string|null}
 */
function validatePutBase(options) {
	if (!options || typeof options !== 'object' || !('base' in options)) {
		throw new TypeError('IndexedDBStore.put requires options { base: string | null }');
	}
	const { base } = options;
	if (base !== null && typeof base !== 'string') {
		throw new TypeError('IndexedDBStore.put base must be a revision string or null');
	}
	return base;
}

/**
 * @implements {import('./types.js').DocumentStore}
 */
export class IndexedDBStore {
	id = 'local';
	label = 'This Browser';
	canShare = false;

	/** @type {Promise<IDBDatabase>|null} */
	#dbPromise = null;
	/** @type {Set<(event: import('./types.js').DocumentChangeEvent) => void>} */
	#watchers = new Set();
	/** @type {BroadcastChannel|null} */
	#rxChannel = null;

	/** @returns {Promise<IDBDatabase>} */
	#getDB() {
		if (!this.#dbPromise) {
			this.#dbPromise = openDB();
		}
		return this.#dbPromise;
	}

	/**
	 * Notify in-tab watchers and broadcast to other same-origin tabs.
	 * @param {import('./types.js').DocumentChangeEvent} event
	 */
	#emitChange(event) {
		for (const fn of this.#watchers) {
			try {
				fn(event);
			} catch {
				// Watcher errors must never fail a committed write.
			}
		}
		if (typeof BroadcastChannel !== 'undefined') {
			try {
				const txChannel = new BroadcastChannel(WATCH_CHANNEL);
				txChannel.postMessage(event);
				txChannel.close();
			} catch {
				// BroadcastChannel unavailable in this environment.
			}
		}
	}

	/**
	 * Subscribe to changes committed to this store (in this tab or another tab).
	 * @param {(event: import('./types.js').DocumentChangeEvent) => void} fn
	 * @returns {() => void}
	 */
	watch(fn) {
		this.#watchers.add(fn);
		if (!this.#rxChannel && typeof BroadcastChannel !== 'undefined') {
			try {
				this.#rxChannel = new BroadcastChannel(WATCH_CHANNEL);
				/** @type {any} */ (this.#rxChannel).unref?.();
				this.#rxChannel.onmessage = (msg) => {
					const event = msg?.data;
					if (!event || event.provider !== this.id) return;
					for (const listener of this.#watchers) {
						try {
							listener(event);
						} catch {
							// Ignore listener errors.
						}
					}
				};
			} catch {
				this.#rxChannel = null;
			}
		}
		return () => {
			this.#watchers.delete(fn);
			if (this.#watchers.size === 0 && this.#rxChannel) {
				this.#rxChannel.close();
				this.#rxChannel = null;
			}
		};
	}

	/**
	 * List all documents, sorted by modified desc.
	 * @returns {Promise<import('./types.js').DocumentSummary[]>}
	 */
	async list() {
		const db = await this.#getDB();
		return new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, 'readonly');
			const store = tx.objectStore(STORE_NAME);
			const request = store.getAll();
			request.onsuccess = () => {
				const docs = request.result.map((doc) => {
					let name = 'Untitled';
					let tabCount = 1;
					let displayUnit = null;
					let previewMesh = null;
					try {
						const parsed = JSON.parse(doc.json);
						if (parsed.document) {
							name = parsed.document.name || name;
							displayUnit = parsed.document.display_unit || null;
						} else if (parsed.project) {
							name = parsed.project.name || name;
							displayUnit = parsed.project.display_unit || null;
						}
						if (parsed.tabs) {
							tabCount = parsed.tabs.length;
							// Extract preview mesh from first tab with one
							for (const tab of parsed.tabs) {
								if (tab.kind?.preview_mesh) {
									previewMesh = tab.kind.preview_mesh;
									break;
								}
							}
						}
					} catch {
						// ignore parse errors for listing
					}
					return {
						id: doc.id,
						name,
						created: doc.created,
						modified: doc.modified,
						displayUnit,
						tabCount,
						previewMesh,
						provider: 'local',
						// Share-link provenance (open-link.js); null for the user's own docs.
						link: doc.link ?? null
					};
				});
				// Sort by modified descending
				docs.sort((a, b) => b.modified - a.modified);
				resolve(docs);
			};
			request.onerror = () => reject(request.error);
		});
	}

	/**
	 * Read the raw IndexedDB object by key.
	 * @param {IDBDatabase} db
	 * @param {string} key
	 * @returns {Promise<any|null>}
	 */
	#readRawByKey(db, key) {
		return new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, 'readonly');
			const request = tx.objectStore(STORE_NAME).get(key);
			request.onsuccess = () => resolve(request.result || null);
			request.onerror = () => reject(request.error);
		});
	}

	/**
	 * Get a document and its revision by its record key, or — since v4 Phase 2
	 * P2-5 — by its `document.id`: records created since then are keyed by the
	 * document's own identity, and older 8-char-keyed records are still found by
	 * the identity inside their JSON (no re-keying migration; a linked read-only
	 * copy of someone else's document is only returned when no record of the
	 * user's own carries that identity).
	 * @param {string} id
	 * @returns {Promise<import('./types.js').DocumentRecord|null>}
	 */
	async get(id) {
		const db = await this.#getDB();
		let raw = await this.#readRawByKey(db, id);
		if (!raw && UUID_RE.test(id)) {
			const all = await new Promise((resolve, reject) => {
				const tx = db.transaction(STORE_NAME, 'readonly');
				const request = tx.objectStore(STORE_NAME).getAll();
				request.onsuccess = () => resolve(request.result || []);
				request.onerror = () => reject(request.error);
			});
			let linkedMatch = null;
			for (const candidate of all) {
				let docId = null;
				try {
					docId = JSON.parse(candidate.json)?.document?.id ?? null;
				} catch {
					continue;
				}
				if (docId !== id) continue;
				if (!candidate.link) {
					raw = candidate;
					break;
				}
				linkedMatch = linkedMatch ?? candidate;
			}
			if (!raw) raw = linkedMatch;
		}
		if (!raw) return null;
		const rev =
			typeof raw.rev === 'string' && raw.rev ? raw.rev : await gitBlobSha1(raw.json);
		return { doc: toStoredDocument(raw), rev };
	}

	/**
	 * Create or update a document at `options.base`. Refuses (never writes) when
	 * the stored revision differs from `options.base`.
	 *
	 * All SHA-1 digests are computed before opening the `readwrite` transaction
	 * because `await crypto.subtle.digest(...)` yields outside IndexedDB's
	 * microtask queue and would auto-commit an open transaction in browsers.
	 * Inside the single `readwrite` transaction, `get` + compare + `put` execute
	 * atomically and are serialized across tabs by IndexedDB.
	 *
	 * @param {import('./types.js').StoredDocument} doc
	 * @param {import('./types.js').DocumentPutOptions} options
	 * @returns {Promise<import('./types.js').DocumentPutResult>}
	 */
	async put(doc, options) {
		const base = validatePutBase(options);
		const db = await this.#getDB();
		const nextRev = await gitBlobSha1(doc.json);

		for (let attempt = 0; attempt < 2; attempt++) {
			const pre = await this.#readRawByKey(db, doc.id);
			let preLegacyJson = null;
			let preLegacyRev = null;
			if (pre && (typeof pre.rev !== 'string' || !pre.rev)) {
				preLegacyJson = pre.json;
				preLegacyRev = await gitBlobSha1(pre.json);
			}

			const outcome = await new Promise((resolve, reject) => {
				const tx = db.transaction(STORE_NAME, 'readwrite');
				const store = tx.objectStore(STORE_NAME);
				/** @type {{ retry?: true, result?: import('./types.js').DocumentPutResult, wrote?: boolean }} */
				let state = {};

				const getReq = store.get(doc.id);
				getReq.onerror = () => reject(getReq.error);
				getReq.onsuccess = () => {
					const current = getReq.result || null;
					let storedRev = null;
					if (current) {
						if (typeof current.rev === 'string' && current.rev) {
							storedRev = current.rev;
						} else if (preLegacyRev !== null && current.json === preLegacyJson) {
							storedRev = preLegacyRev;
						} else {
							// Another writer inserted a legacy record without `.rev` mid-digest; re-read once.
							state = { retry: true };
							return;
						}
					}

					if (base !== storedRev) {
						state = { result: { conflict: { theirs_rev: storedRev } }, wrote: false };
						return;
					}

					const record = {
						id: doc.id,
						json: doc.json,
						created: current?.created ?? doc.created,
						modified: doc.modified,
						...(doc.link !== undefined
							? doc.link !== null
								? { link: doc.link }
								: {}
							: current?.link !== undefined
								? { link: current.link }
								: {}),
						...(doc.localFallback !== undefined
							? doc.localFallback
								? { localFallback: true }
								: {}
							: current?.localFallback
								? { localFallback: true }
								: {}),
						rev: nextRev
					};
					const putReq = store.put(record);
					putReq.onerror = () => reject(putReq.error);
					state = { result: { rev: nextRev }, wrote: true };
				};

				tx.oncomplete = () => resolve(state);
				tx.onerror = () => reject(tx.error);
				tx.onabort = () => reject(tx.error || new Error('IndexedDB put transaction aborted'));
			});

			if (outcome.retry) continue;
			if (outcome.wrote) {
				this.#emitChange({
					type: 'changed',
					id: doc.id,
					rev: nextRev,
					provider: this.id
				});
			}
			return /** @type {import('./types.js').DocumentPutResult} */ (outcome.result);
		}

		throw new Error('IndexedDBStore.put could not determine stored revision after retry');
	}

	/**
	 * Delete a document by ID.
	 * @param {string} id
	 * @returns {Promise<void>}
	 */
	async delete(id) {
		const db = await this.#getDB();
		await new Promise((resolve, reject) => {
			const tx = db.transaction(STORE_NAME, 'readwrite');
			const store = tx.objectStore(STORE_NAME);
			const request = store.delete(id);
			request.onerror = () => reject(request.error);
			tx.oncomplete = () => resolve();
			tx.onerror = () => reject(tx.error);
		});
		this.#emitChange({
			type: 'deleted',
			id,
			rev: null,
			provider: this.id
		});
	}

	/**
	 * Get a shareable URL for a document (not supported for local storage).
	 * @param {string} _id
	 * @returns {Promise<string|null>}
	 */
	async getShareUrl(_id) {
		return null;
	}
}
