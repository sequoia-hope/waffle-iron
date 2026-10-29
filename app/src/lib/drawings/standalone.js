/**
 * The exported sheet SVG, standing alone (REVIEW.md §3.3).
 *
 * On screen a sheet paints with `var(--drawing-…)` and `var(--font-ui)` so it
 * follows the theme (`style.js`). A `.svg` file opened anywhere else has no
 * stylesheet to resolve them against: an unresolvable `var()` paint is
 * invalid, so every stroke vanishes and every fill falls back to black. The
 * export therefore writes literals — the SAME print palette the PDF uses
 * (`pdf.js` `PRINT_PALETTE`: black ink on white paper, a sheet is paper), and
 * Helvetica, whose metrics the PDF writer lays text out with.
 */
import { PRINT_PALETTE } from './pdf.js';

/** The font a standalone sheet names (the PDF writes Helvetica too). */
export const PRINT_FONT_FAMILY = 'Helvetica, Arial, sans-serif';

/** `[r, g, b]` in 0–1 as `#rrggbb`. */
function hex(rgb) {
	return '#' + rgb.map((c) => Math.round(Math.min(1, Math.max(0, c)) * 255).toString(16).padStart(2, '0')).join('');
}

/**
 * Replace every `var(--token)` in a rendered sheet with a literal.
 *
 * A token the palette does not know is written black and NAMED in the
 * warnings rather than left as `var()` — a renderer that grows a new token
 * gets a visible line and a warning, never an invisible one.
 *
 * @param {string} svg - markup from `renderSheetSvg`
 * @param {{ palette?: Record<string, number[]>, fontFamily?: string }} [opts]
 * @returns {{ svg: string, warnings: string[] }}
 */
export function standaloneSheetSvg(svg, { palette = PRINT_PALETTE, fontFamily = PRINT_FONT_FAMILY } = {}) {
	/** @type {Set<string>} */
	const unknown = new Set();
	const out = svg.replace(/var\(\s*(--[A-Za-z0-9-]+)\s*(?:,[^)]*)?\)/g, (_m, name) => {
		if (name === '--font-ui' || name === '--font-mono') return fontFamily;
		const rgb = palette[name];
		if (rgb) return hex(rgb);
		unknown.add(name);
		return '#000000';
	});
	const warnings = [...unknown].map((name) => `the paint var(${name}) has no print colour and was written as black`);
	return { svg: out, warnings };
}
