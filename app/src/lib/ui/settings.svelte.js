/**
 * Application settings store (Svelte 5 runes).
 *
 * One reactive object, persisted to localStorage under `waffle:settings`.
 * Every key has a default in SETTINGS_DEFAULTS; unknown keys in storage are
 * ignored, missing keys fall back to their default, so adding a setting is a
 * one-line change here plus a control in SettingsModal.svelte.
 *
 * Color customization: `colors` maps a CSS custom-property name (see
 * COLOR_TOKENS) to a hex value. Overrides are applied as inline styles on
 * <html>, so they layer on top of whichever base theme (theme.svelte.js) is
 * selected. `colorVersion` ticks whenever colors change so canvas renderers
 * that read tokens through getComputedStyle (the sketch renderer) re-read.
 */

import { getTheme, setTheme, THEMES } from './theme.svelte.js';

const STORAGE_KEY = 'waffle:settings';

export const SETTINGS_DEFAULTS = Object.freeze({
	/** Extrude dialog pre-selects a region of the last sketch when it opens. */
	extrudeAutoSelectRegion: false,
	/** The FIRST driving dimension on a sketch scales the whole sketch to it. */
	sketchScaleOnFirstDimension: true,
	/** Inline CSS-token overrides: { '--accent': '#rrggbb', ... }. */
	colors: {},
});

/**
 * Every customizable color token, grouped for the settings UI. `id` is the
 * CSS custom property; `label` is what the user sees. Defaults live in
 * app.css (`:root`) and the per-theme override blocks, so the base value shown
 * in the editor is read from the computed style of <html>.
 */
export const COLOR_TOKENS = [
	{ group: 'Interface', tokens: [
		{ id: '--bg-primary', label: 'Background' },
		{ id: '--bg-secondary', label: 'Panel background' },
		{ id: '--bg-tertiary', label: 'Control background' },
		{ id: '--bg-hover', label: 'Hover background' },
		{ id: '--text-primary', label: 'Text' },
		{ id: '--text-secondary', label: 'Secondary text' },
		{ id: '--text-muted', label: 'Muted text' },
		{ id: '--border-color', label: 'Borders' },
		{ id: '--accent', label: 'Accent' },
		{ id: '--accent-hover', label: 'Accent hover' },
		{ id: '--text-on-accent', label: 'Text on accent' },
		{ id: '--success', label: 'Success' },
		{ id: '--warning', label: 'Warning' },
		{ id: '--error', label: 'Error' },
	] },
	{ group: 'Viewport', tokens: [
		{ id: '--viewport-bg', label: 'Viewport background' },
		{ id: '--model-color', label: 'Model faces' },
	] },
	{ group: 'Sketch', tokens: [
		{ id: '--sketch-default', label: 'Entities (under-constrained)' },
		{ id: '--sketch-constrained', label: 'Entities (fully constrained)' },
		{ id: '--sketch-construction', label: 'Construction geometry' },
		{ id: '--sketch-selected', label: 'Selected' },
		{ id: '--sketch-hovered', label: 'Hovered' },
		{ id: '--sketch-preview', label: 'Drawing preview' },
		{ id: '--sketch-snap', label: 'Snap indicator' },
		{ id: '--sketch-overconstrained', label: 'Over-constrained' },
		{ id: '--sketch-profile-hover', label: 'Region hover' },
		{ id: '--sketch-profile-select', label: 'Region selected' },
	] },
];

/** Flat list of token ids, for validation. */
const TOKEN_IDS = new Set(COLOR_TOKENS.flatMap((g) => g.tokens.map((t) => t.id)));

const HEX_RE = /^#([0-9a-f]{6}|[0-9a-f]{8}|[0-9a-f]{3})$/i;

/** @param {any} v */
function isHex(v) {
	return typeof v === 'string' && HEX_RE.test(v.trim());
}

/**
 * Keep only known tokens with valid hex values.
 * @param {any} colors
 * @returns {Record<string, string>}
 */
function sanitizeColors(colors) {
	/** @type {Record<string, string>} */
	const out = {};
	if (!colors || typeof colors !== 'object') return out;
	for (const [k, v] of Object.entries(colors)) {
		if (TOKEN_IDS.has(k) && isHex(v)) out[k] = v.trim().toLowerCase();
	}
	return out;
}

/**
 * Merge a raw (possibly partial / stale) object onto the defaults.
 * @param {any} raw
 */
function normalize(raw) {
	const s = { ...SETTINGS_DEFAULTS, colors: {} };
	if (raw && typeof raw === 'object') {
		for (const k of Object.keys(SETTINGS_DEFAULTS)) {
			if (k === 'colors') continue;
			if (k in raw && typeof raw[k] === typeof SETTINGS_DEFAULTS[k]) s[k] = raw[k];
		}
		s.colors = sanitizeColors(raw.colors);
	}
	return s;
}

function readStored() {
	if (typeof localStorage === 'undefined') return normalize(null);
	try {
		const v = localStorage.getItem(STORAGE_KEY);
		return normalize(v ? JSON.parse(v) : null);
	} catch {
		return normalize(null);
	}
}

let settings = $state(readStored());
let colorVersion = $state(0);

function persist() {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
	} catch {
		// Storage unavailable: settings still apply for the session.
	}
}

/** Write the color overrides onto <html> as inline custom properties. */
function applyColorsToDocument() {
	if (typeof document === 'undefined') return;
	const style = document.documentElement.style;
	for (const id of TOKEN_IDS) {
		const v = settings.colors[id];
		if (v) style.setProperty(id, v);
		else style.removeProperty(id);
	}
	colorVersion++;
}

/** @returns {typeof SETTINGS_DEFAULTS} the live settings (reactive) */
export function getSettings() {
	return settings;
}

/**
 * @template {keyof typeof SETTINGS_DEFAULTS} K
 * @param {K} key
 * @returns {(typeof SETTINGS_DEFAULTS)[K]}
 */
export function getSetting(key) {
	return settings[key];
}

/**
 * Update one or more settings. Unknown keys are ignored.
 * @param {Partial<typeof SETTINGS_DEFAULTS>} patch
 */
export function updateSettings(patch) {
	const next = normalize({ ...settings, ...patch });
	const colorsChanged = JSON.stringify(next.colors) !== JSON.stringify(settings.colors);
	settings = next;
	persist();
	if (colorsChanged) applyColorsToDocument();
}

/**
 * Set (or clear with null) one color override.
 * @param {string} tokenId
 * @param {string | null} hex
 */
export function setColorOverride(tokenId, hex) {
	if (!TOKEN_IDS.has(tokenId)) return;
	const colors = { ...settings.colors };
	if (hex && isHex(hex)) colors[tokenId] = hex.trim().toLowerCase();
	else delete colors[tokenId];
	updateSettings({ colors });
}

/** Remove every color override (back to the base theme). */
export function clearColorOverrides() {
	updateSettings({ colors: {} });
}

/** Reset every setting (including colors) to its default. */
export function resetSettings() {
	updateSettings({ ...SETTINGS_DEFAULTS, colors: {} });
}

/**
 * Ticks whenever color overrides (or the base theme via applyColorScheme)
 * change. Canvas renderers read it to re-sample tokens.
 * @returns {number}
 */
export function getColorVersion() {
	return colorVersion;
}

/** Bump the color version (call after a base-theme switch). */
export function bumpColorVersion() {
	colorVersion++;
}

/**
 * Current computed value of a color token on <html> (override or theme
 * baseline). Empty string when unavailable (SSR).
 * @param {string} tokenId
 */
export function readTokenValue(tokenId) {
	if (typeof document === 'undefined') return '';
	return getComputedStyle(document.documentElement).getPropertyValue(tokenId).trim();
}

/**
 * The COMPLETE color scheme as a portable JSON string: base theme + every
 * token's effective value. Pasting it into another install reproduces the
 * look exactly (every token is listed, so a differing base theme elsewhere
 * cannot leak through).
 * @returns {string}
 */
export function exportColorScheme() {
	/** @type {Record<string, string>} */
	const colors = {};
	for (const id of TOKEN_IDS) {
		const v = settings.colors[id] || readTokenValue(id);
		if (v) colors[id] = v;
	}
	return JSON.stringify({ format: 'waffle-color-scheme', version: 1, theme: getTheme(), colors }, null, 2);
}

/**
 * Apply a pasted color scheme. Accepts the export format (with `theme` and
 * `colors`) or a bare `{ '--token': '#hex' }` map. Returns an error string
 * when the text is not a usable scheme.
 * @param {string} text
 * @returns {string | null}
 */
export function importColorScheme(text) {
	let parsed;
	try {
		parsed = JSON.parse(text);
	} catch {
		return 'Not valid JSON';
	}
	if (!parsed || typeof parsed !== 'object') return 'Expected a JSON object';
	const colorsRaw = parsed.colors && typeof parsed.colors === 'object' ? parsed.colors : parsed;
	const colors = sanitizeColors(colorsRaw);
	if (Object.keys(colors).length === 0) return 'No recognized color tokens found';
	if (typeof parsed.theme === 'string' && THEMES.some((t) => t.id === parsed.theme)) {
		setTheme(parsed.theme);
	}
	updateSettings({ colors });
	return null;
}

/**
 * Reconcile the store with persisted values and apply color overrides to the
 * document. Call once on app mount (after initTheme()).
 */
export function initSettings() {
	settings = readStored();
	applyColorsToDocument();
}
