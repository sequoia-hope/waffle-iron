/**
 * Theme store (Svelte 5 runes).
 *
 * A theme is a set of values for the CSS custom properties defined in
 * app.css. Selecting a theme sets `data-theme` on <html>; the matching
 * `:root[data-theme='<id>']` block in app.css then overrides the color/font
 * tokens. The choice is persisted to localStorage.
 *
 * The DEFAULT theme has no override block — it is the unconditional `:root`
 * baseline — so `data-theme='default'` simply means "no overrides apply".
 *
 * To add a theme: add its `:root[data-theme='<id>']` block to app.css and a
 * matching entry to THEMES below. Nothing else needs to change.
 */

const STORAGE_KEY = 'waffle:theme';

/**
 * Registered themes, in the order they appear in the switcher.
 * `id` must match the `data-theme` selector in app.css (default = baseline).
 * @type {ReadonlyArray<{ id: string, label: string, description: string }>}
 */
export const THEMES = [
	{ id: 'default', label: 'Default', description: 'The standard dark editor theme' },
	{ id: 'retro', label: 'Retro Terminal', description: 'Grey, black, and old-terminal phosphor green' },
	{ id: 'witchhazel', label: 'Witch Hazel', description: 'Dark violet with lavender, mint, and pink accents' },
];

const DEFAULT_THEME = 'default';

/** @param {string} id */
function isKnown(id) {
	return THEMES.some((t) => t.id === id);
}

/**
 * Read the persisted theme id, falling back to the default. Safe to call
 * during SSR (returns the default when there is no window).
 * @returns {string}
 */
function readStored() {
	if (typeof localStorage === 'undefined') return DEFAULT_THEME;
	try {
		const v = localStorage.getItem(STORAGE_KEY);
		return v && isKnown(v) ? v : DEFAULT_THEME;
	} catch {
		return DEFAULT_THEME;
	}
}

let current = $state(readStored());

/**
 * Apply `id` to the document root so app.css picks up the override block.
 * @param {string} id
 */
function applyToDocument(id) {
	if (typeof document !== 'undefined') {
		document.documentElement.dataset.theme = id;
	}
}

/** @returns {string} the active theme id */
export function getTheme() {
	return current;
}

/** @returns {{ id: string, label: string, description: string } | undefined} */
export function getThemeMeta() {
	return THEMES.find((t) => t.id === current);
}

/**
 * Select a theme: update reactive state, apply it to <html>, and persist it.
 * Unknown ids fall back to the default.
 * @param {string} id
 */
export function setTheme(id) {
	const next = isKnown(id) ? id : DEFAULT_THEME;
	current = next;
	applyToDocument(next);
	if (typeof localStorage !== 'undefined') {
		try {
			localStorage.setItem(STORAGE_KEY, next);
		} catch {
			// Private-mode / disabled storage: the theme still applies for the
			// session, it just won't persist. Not worth surfacing to the user.
		}
	}
}

/**
 * Sync the document + store to the persisted value. The anti-flash script in
 * app.html already sets `data-theme` before first paint; this reconciles the
 * runes store with that value once the app mounts (and is a no-op re-apply if
 * they already agree).
 */
export function initTheme() {
	const stored = readStored();
	current = stored;
	applyToDocument(stored);
}
