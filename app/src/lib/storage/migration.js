import { generateDocId } from './types.js';

/**
 * Migrate the legacy `waffle-autosave` localStorage entry to IndexedDB.
 * Reads the localStorage key, parses it, saves to the store, then removes the key.
 *
 * @param {import('./types.js').DocumentStore} store
 * @returns {Promise<string|null>} The new document ID if migration occurred, null otherwise
 */
export async function migrateLocalStorage(store) {
	const key = 'waffle-autosave';
	const raw = localStorage.getItem(key);
	if (!raw) return null;

	try {
		// Validate it's parseable JSON
		JSON.parse(raw);
	} catch {
		// Corrupt data — remove and skip
		localStorage.removeItem(key);
		return null;
	}

	const id = generateDocId();
	const now = Date.now();
	await store.put({
		id,
		json: raw,
		created: now,
		modified: now
	});

	localStorage.removeItem(key);
	return id;
}
