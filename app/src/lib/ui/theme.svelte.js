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
 * matching entry to THEMES below. Nothing else needs to change — every
 * surface that lists themes (the switcher, Settings -> Appearance) iterates
 * THEMES. See the header of app.css for the two rules the block must follow
 * (declare `color-scheme`; a light theme redefines every color token).
 */

const STORAGE_KEY = 'waffle:theme';

/**
 * Registered themes, in the order they appear in the switcher — light/dark
 * pairs are kept adjacent. `id` must match the `data-theme` selector in
 * app.css (default = baseline). `appearance` says which way the theme leans;
 * it mirrors the `color-scheme` in that block and is what the browser-chrome
 * color falls back to before the document has been styled.
 * @type {ReadonlyArray<{ id: string, label: string, description: string, appearance: 'dark' | 'light' }>}
 */
export const THEMES = [
	{ id: 'default', label: 'Default', description: 'The standard dark editor theme', appearance: 'dark' },
	{ id: 'light', label: 'Light', description: 'The standard theme on a bright ground', appearance: 'light' },
	{ id: 'solarized-light', label: 'Solarized Light', description: "Schoonover's warm cream ground with the canonical accents", appearance: 'light' },
	{ id: 'solarized-dark', label: 'Solarized Dark', description: 'The same accents on deep blue-green base03', appearance: 'dark' },
	{ id: 'monokai-light', label: 'Monokai Light', description: 'Monokai inverted onto its own warm off-white foreground', appearance: 'light' },
	{ id: 'monokai-dark', label: 'Monokai Dark', description: 'The classic olive-black ground with cyan, green, and pink', appearance: 'dark' },
	{ id: 'retro', label: 'Retro Terminal', description: 'Grey, black, and old-terminal phosphor green', appearance: 'dark' },
	{ id: 'witchhazel', label: 'Witch Hazel', description: 'Dark violet with lavender, mint, and pink accents', appearance: 'dark' },
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
 * Keep <meta name="theme-color"> on the active theme's page background, so the
 * mobile browser chrome (and the iOS status bar, which app.html declares
 * translucent) matches instead of staying the hard-coded dark of the default
 * theme. Reads the token back off <html> so a per-token override from
 * Settings -> Appearance is honoured too.
 * @param {string} id
 */
function applyBrowserChromeColor(id) {
	const meta = document.querySelector('meta[name="theme-color"]');
	if (!meta) return;
	const bg = getComputedStyle(document.documentElement).getPropertyValue('--bg-primary').trim();
	if (bg) {
		meta.setAttribute('content', bg);
		return;
	}
	// Unstyled document (the stylesheet has not landed yet): fall back to the
	// registered appearance rather than leaving the previous theme's color.
	const entry = THEMES.find((t) => t.id === id);
	meta.setAttribute('content', entry?.appearance === 'light' ? '#ffffff' : '#1e1e2e');
}

/**
 * Apply `id` to the document root so app.css picks up the override block.
 * @param {string} id
 */
function applyToDocument(id) {
	if (typeof document !== 'undefined') {
		document.documentElement.dataset.theme = id;
		applyBrowserChromeColor(id);
	}
}

/** @returns {string} the active theme id */
export function getTheme() {
	return current;
}

/** @returns {(typeof THEMES)[number] | undefined} */
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
