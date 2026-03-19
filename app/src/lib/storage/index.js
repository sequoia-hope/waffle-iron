import { IndexedDBStore } from './indexeddb.js';

export { generateDocId } from './types.js';
export { IndexedDBStore } from './indexeddb.js';
export { migrateLocalStorage } from './migration.js';

/** @type {IndexedDBStore|null} */
let _store = null;

/**
 * Get the singleton IndexedDB store instance.
 * @returns {IndexedDBStore}
 */
export function getStore() {
	if (!_store) {
		_store = new IndexedDBStore();
	}
	return _store;
}
