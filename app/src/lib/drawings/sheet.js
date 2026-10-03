/**
 * One drawing SHEET as SVG (`specs/drawings_and_mbd.md` §8, D4a): a piece of
 * paper with the sheet's views placed on it.
 *
 * Composition by NESTED `<svg>`, one per view. Each view is rendered by
 * `renderViewSvg` exactly as it is on its own — same function, same bytes —
 * and the nesting only positions it, so the markup a drafter sees in one view
 * and the markup on the sheet are the same string. The alternative (a second
 * renderer that lays views out in one coordinate system) would be a second
 * source of truth for the same geometry, which is the mistake
 * `DrawingView.svelte` already refuses for a single view.
 *
 * Units: one SVG user unit is one paper millimetre, as in `layout.js`, and the
 * sheet's `viewBox` is its paper size with `width`/`height` in `mm`, so the
 * browser's print path is true to scale at any sheet size.
 *
 * `y` DOWN on paper, `+y` UP in the document. `DrawingView.placement_mm` is
 * measured from the sheet's bottom-left corner (the drafting convention a
 * title block's coordinates use), and SVG measures from the top-left, so the
 * placement is flipped here — once, in one expression, rather than by a
 * `scale(1, -1)` transform that would mirror every label.
 */
import { DRAWING_TOKENS } from './style.js';
import { esc, n, renderViewSvg } from './svg.js';

/** The paper border's inset from the sheet edge, in millimetres (ISO 5457). */
export const SHEET_MARGIN_MM = 10;

/**
 * One view's standalone SVG, re-sized and positioned as a NESTED `<svg>` on
 * the sheet: `x`/`y` plus a width and height in the sheet's own user units
 * (paper mm).
 *
 * The view's own `width`/`height` are REMOVED rather than shadowed by the new
 * ones. Two `width` attributes on one element is a fatal XML
 * well-formedness error (XML 1.0 "Unique Att Spec"), so an exported `.svg`
 * built that way does not open in any SVG tool at all — while the browser's
 * HTML tokenizer silently keeps the first of the pair, which is why the
 * screen looked right. Measured: `xml.etree` refused the sheet with
 * "duplicate attribute".
 *
 * `null` when there is no element to place.
 *
 * @param {string} svg one view's SVG, as `renderViewSvg` returns it
 * @param {{ x: number, y: number, widthMm: number, heightMm: number }} box
 * @returns {string | null}
 */
export function placedIn(svg, { x, y, widthMm, heightMm }) {
	const close = String(svg ?? '').indexOf('>');
	if (close < 0) return null;
	const openTag = svg.slice(0, close).replace(/\s+(?:width|height)="[^"]*"/g, '');
	return (
		`${openTag} x="${n(x)}" y="${n(y)}" ` +
		`width="${n(widthMm)}" height="${n(heightMm)}"` +
		svg.slice(close)
	);
}

/**
 * Render a sheet.
 *
 * @param {object} input
 * @param {any} input.sheet a `Sheet`: `{ id, name, size, orientation, views }`
 *   with each view carrying its `cache` (a `ViewLayout`)
 * @param {string} [input.unit] display unit for dimension text
 * @param {number} [input.documentPrecision]
 * @param {Partial<import('./style.js').DrawingStyle>} [input.style]
 * @param {boolean} [input.border] draw the sheet's border frame (default true)
 * @returns {{ svg: string, widthMm: number, heightMm: number, views: number,
 *            warnings: string[] }}
 */
export function renderSheetSvg({ sheet, unit = 'mm', documentPrecision = 2, style, border = true }) {
	const warnings = [];
	const [widthMm, heightMm] = sheetExtentMm(sheet);
	const views = sheet?.views ?? [];

	const parts = [];
	let drawn = 0;
	for (const view of views) {
		const layout = view?.cache;
		if (!layout) {
			// A view the engine could not rebuild. Named, not skipped
			// silently: a sheet with a missing view must say so, because the
			// drawing LOOKS complete without it.
			warnings.push(`view "${view?.name ?? '?'}" has no layout and was not drawn`);
			continue;
		}
		const rendered = renderViewSvg({
			layout,
			scale: view.scale ?? 1,
			style,
			unit,
			documentPrecision,
			title: view.name ?? null,
			paper: false
		});
		warnings.push(...rendered.warnings.map((w) => `view "${view?.name ?? '?'}": ${w}`));
		// The view's own SVG is `rendered.widthMm × rendered.heightMm` with
		// the drawing centred in it (the margin is symmetric), so placing its
		// CENTRE at the view's placement is placing the drawing there.
		const placement = view.placement_mm ?? [widthMm / 2, heightMm / 2];
		const x = Number(placement[0]) - rendered.widthMm / 2;
		// The flip: a placement measured up from the bottom becomes a top edge
		// measured down from the top.
		const y = heightMm - Number(placement[1]) - rendered.heightMm / 2;
		if (![x, y].every(Number.isFinite)) {
			warnings.push(`view "${view?.name ?? '?'}" has a placement that is not two numbers`);
			continue;
		}
		const placed = placedIn(rendered.svg, {
			x,
			y,
			widthMm: rendered.widthMm,
			heightMm: rendered.heightMm
		});
		if (placed === null) {
			warnings.push(`view "${view?.name ?? '?'}" did not render an element to place`);
			continue;
		}
		parts.push(
			`<g class="wi-sheet-view" data-view-id="${esc(view.id ?? '')}" data-view-name="${esc(view.name ?? '')}">` +
				placed +
				`</g>`
		);
		drawn += 1;
	}

	const borderEl = border
		? `<rect class="wi-sheet-border" x="${n(SHEET_MARGIN_MM)}" y="${n(SHEET_MARGIN_MM)}" ` +
			`width="${n(widthMm - 2 * SHEET_MARGIN_MM)}" height="${n(heightMm - 2 * SHEET_MARGIN_MM)}" ` +
			`fill="none" stroke="${DRAWING_TOKENS.annotation}" stroke-width="0.5" />`
		: '';

	const svg =
		`<svg xmlns="http://www.w3.org/2000/svg" class="wi-sheet" ` +
		`width="${n(widthMm)}mm" height="${n(heightMm)}mm" ` +
		`viewBox="0 0 ${n(widthMm)} ${n(heightMm)}" ` +
		`data-sheet-id="${esc(sheet?.id ?? '')}" data-views="${drawn}">` +
		`<title>${esc(sheet?.name ?? 'Sheet')}</title>` +
		`<rect class="wi-paper" x="0" y="0" width="${n(widthMm)}" height="${n(heightMm)}" fill="${DRAWING_TOKENS.paper}" />` +
		borderEl +
		parts.join('') +
		`</svg>`;

	return { svg, widthMm, heightMm, views: drawn, warnings };
}

/**
 * The sheet's `[width, height]` in millimetres.
 *
 * The engine sends `extent_mm` with the orientation already applied; the table
 * here is the fallback for a sheet that arrived without it (a document written
 * by a build that did not send the field). Deriving it twice is how the UI and
 * the engine come to disagree about the paper size, so the engine's number
 * wins whenever it is there.
 * @param {any} sheet
 */
export function sheetExtentMm(sheet) {
	const given = sheet?.extent_mm;
	if (Array.isArray(given) && given.length === 2 && given.every((v) => Number.isFinite(v) && v > 0)) {
		return [Number(given[0]), Number(given[1])];
	}
	const portrait = PORTRAIT_MM[sheet?.size?.type] ?? null;
	if (!portrait) {
		const w = Number(sheet?.size?.width_mm);
		const h = Number(sheet?.size?.height_mm);
		if (Number.isFinite(w) && Number.isFinite(h) && w > 0 && h > 0) return [w, h];
		return [...PORTRAIT_MM.A3].reverse();
	}
	return sheet?.orientation?.type === 'Portrait' ? [...portrait] : [...portrait].reverse();
}

/** Portrait `[width, height]` in millimetres, mirroring `SheetSize`. */
const PORTRAIT_MM = {
	A4: [210, 297],
	A3: [297, 420],
	A2: [420, 594],
	A1: [594, 841],
	A0: [841, 1189],
	Letter: [215.9, 279.4],
	Tabloid: [279.4, 431.8]
};
