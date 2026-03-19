import { IndexedDBStore } from './indexeddb.js';

export { generateDocId } from './types.js';
export { IndexedDBStore } from './indexeddb.js';
export { migrateLocalStorage } from './migration.js';

/** @type {Map<string, import('./types.js').DocumentStore>} */
const _providers = new Map();

let _activeProviderId = 'local';

// Register local provider on import
_providers.set('local', new IndexedDBStore());

// Restore active provider preference
if (typeof localStorage !== 'undefined') {
	const saved = localStorage.getItem('waffle-active-provider');
	if (saved) _activeProviderId = saved;
}

/**
 * Register a storage provider.
 * @param {import('./types.js').DocumentStore} store
 */
export function registerProvider(store) {
	_providers.set(store.id, store);
}

/**
 * Unregister a storage provider.
 * @param {string} id
 */
export function unregisterProvider(id) {
	if (id === 'local') return; // Can't remove local
	_providers.delete(id);
	if (_activeProviderId === id) {
		setActiveProvider('local');
	}
}

/**
 * Get a provider by ID.
 * @param {string} id
 * @returns {import('./types.js').DocumentStore|null}
 */
export function getProvider(id) {
	return _providers.get(id) ?? null;
}

/**
 * Get the active provider (defaults to local).
 * @returns {import('./types.js').DocumentStore}
 */
export function getActiveProvider() {
	return _providers.get(_activeProviderId) ?? _providers.get('local');
}

/**
 * Set the active provider.
 * @param {string} id
 */
export function setActiveProvider(id) {
	if (_providers.has(id)) {
		_activeProviderId = id;
		if (typeof localStorage !== 'undefined') {
			localStorage.setItem('waffle-active-provider', id);
		}
	}
}

/**
 * Get the active provider ID.
 * @returns {string}
 */
export function getActiveProviderId() {
	return _activeProviderId;
}

/**
 * Get all registered providers.
 * @returns {import('./types.js').DocumentStore[]}
 */
export function getAllProviders() {
	return [..._providers.values()];
}

/**
 * Legacy compatibility — returns the local store.
 * @deprecated Use getActiveProvider() instead
 * @returns {IndexedDBStore}
 */
export function getStore() {
	return /** @type {IndexedDBStore} */ (_providers.get('local'));
}
