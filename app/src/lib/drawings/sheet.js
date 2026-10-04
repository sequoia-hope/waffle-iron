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
import { DRAWING_TOKENS, drawingStyle } from './style.js';
import { esc, n, renderViewSvg } from './svg.js';

/** The paper border's inset from the sheet edge, in millimetres (ISO 5457). */
export const SHEET_MARGIN_MM = 10;

/**
 * `1:1`, `1:2`, `2:1` — the ratio a drafter reads, from the number.
 *
 * Mirrors `feature_engine::drawing::scale_ratio_label`, which fills the title
 * block's `Scale` row. The two must agree: a sheet whose title block says
 * `1:2` and whose detail caption says something else is a drawing that
 * contradicts itself. Kept in step by being the only copy on this side —
 * `DrawingPanel` imports this rather than carrying its own, which it did until
 * the detail caption needed a third.
 *
 * @param {number} scale paper length per model length
 * @returns {string}
 */
export function scaleRatioLabel(scale) {
	const s = Number(scale);
	if (!Number.isFinite(s) || s <= 0) return '—';
	if (Math.abs(s - 1) < 1e-9) return '1:1';
	const round = (x) => (Math.abs(x - Math.round(x)) < 1e-6 ? String(Math.round(x)) : x.toFixed(2));
	return s < 1 ? `1:${round(1 / s)}` : `${round(s)}:1`;
}

/**
 * The designation drawn under a view, or `null` for a view that carries none
 * (D4b review).
 *
 * Only the DERIVED kinds are captioned, which is what the standard asks and
 * what a drafter draws. A section and a detail must be identified ON the view
 * — ISO 128-30 puts the letters at the cutting line AND under the view it
 * produced, and a detail has to print its own scale because it is the one view
 * that does not share the sheet's. An orthographic view in a projection group
 * needs no label: its POSITION says which side it shows, and captioning six
 * views FRONT/TOP/RIGHT is clutter that makes the two labels that matter
 * harder to find.
 *
 * The text is the view's own `name` — the engine sets `SECTION A-A` and
 * `DETAIL A` — so this adds no second source of truth for what a view is
 * called, only the decision of whether to draw it. A view the engine named and
 * a person then renamed prints the person's name, which is the point of the
 * field being editable.
 *
 * A DETAIL also gets its ratio appended, and that is not decoration. A detail
 * is the one view that does not share the sheet's scale — the title block's
 * `Scale` row excludes details for exactly that reason, and prints `AS SHOWN`
 * only when the non-detail views disagree — so the enlargement appears nowhere
 * else on the paper. Appended at RENDER time rather than baked into the name,
 * because the name is editable and a stored `(2:1)` would survive a change of
 * scale and then lie. A detail drawn at the sheet's own 1:1 still prints it:
 * `DETAIL A (1:1)` tells a reader it was checked, where a bare `DETAIL A`
 * leaves them to assume.
 *
 * @param {any} view
 * @returns {string|null}
 */
export function viewCaption(view) {
	const kind = view?.projection?.type;
	if (kind !== 'Section' && kind !== 'Detail') return null;
	const name = String(view?.name ?? '').trim();
	if (name.length === 0) return null;
	if (kind !== 'Detail') return name;
	const ratio = scaleRatioLabel(view?.scale ?? 1);
	// An unusable scale prints the name alone rather than `DETAIL A (—)`: the
	// view is already refused by the engine for a non-positive scale, and a
	// caption is not the place to report it.
	return ratio === '—' || name.includes(ratio) ? name : `${name} (${ratio})`;
}

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
			caption: viewCaption(view),
			paper: false,
			// A sheet nests several views in ONE document, so a detail's
			// clipPath id has to be the view's own: two details declaring
			// the same id would both clip to whichever came first (D4b).
			idPrefix: view.id ?? ''
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
	// D4b. After the views, so the title block is never drawn over; it sits
	// in the frame's corner, which the auto-layout does not reserve, so a
	// view placed there would otherwise hide it.
	const titleBlock = renderTitleBlock(sheet, [widthMm, heightMm], drawingStyle(style));
	warnings.push(...titleBlock.warnings);

	const svg =
		`<svg xmlns="http://www.w3.org/2000/svg" class="wi-sheet" ` +
		`width="${n(widthMm)}mm" height="${n(heightMm)}mm" ` +
		`viewBox="0 0 ${n(widthMm)} ${n(heightMm)}" ` +
		`data-sheet-id="${esc(sheet?.id ?? '')}" data-views="${drawn}">` +
		`<title>${esc(sheet?.name ?? 'Sheet')}</title>` +
		`<rect class="wi-paper" x="0" y="0" width="${n(widthMm)}" height="${n(heightMm)}" fill="${DRAWING_TOKENS.paper}" />` +
		borderEl +
		parts.join('') +
		titleBlock.svg +
		`</svg>`;

	return { svg, widthMm, heightMm, views: drawn, warnings };
}

/**
 * The sheet's title block as SVG (`specs/drawings_and_mbd.md` §8, D4b): the
 * data-field block in the frame's bottom-right corner.
 *
 * ## It is DATA, not a layout
 *
 * The rows come from the engine already filled
 * (`Sheet::title_block_cache`, a `TitleBlockLayout` of label/value pairs), so
 * this function decides where the lines go and nothing about what they say —
 * the same division as `renderViewSvg` over a `ViewLayout`. A renderer that
 * worked out the sheet number itself would be a second source of truth for a
 * number the document already knows.
 *
 * That division is also why D4c's EXPRESSION rows needed no change here: a
 * row whose value is `volume(plate) * 0.00785` arrives already evaluated to
 * `7.85 mm³`, and the one thing this renderer must never do is print the
 * source text instead.
 *
 * It is a DISCIPLINE, not an impossibility, and the distinction matters to
 * whoever edits this next. The evaluated rows come from
 * `sheet.title_block_cache.rows`, but this function is handed the whole
 * `sheet`, and `sheet.title_block.fields[i].expr` is the source — sitting one
 * property away. Nothing structural stops a future row renderer from reaching
 * for it (to show a tooltip, say, or to fall back when a row is blank), and a
 * fallback is exactly how a sheet comes to print `volume(plate)` where a
 * drafter expects a number. So the rule is measured rather than assumed:
 * `drawing-tab.spec.js`'s "an expression row prints its value on the paper and
 * its source nowhere" asserts the evaluated text is in the markup and the
 * source string is not, on the screen and in the exported SVG alike.
 *
 * ## The projection standard is printed as WORDS
 *
 * ISO 5456-2's projection symbol is a truncated cone shown in two views, and
 * the first- and third-angle symbols are MIRROR IMAGES of one another — the
 * two concentric circles are identical in both, so the only thing that
 * distinguishes them is which side of the trapezoid they sit on. Which side
 * is which is a convention, and it is not derivable from the projection rule:
 * the circles view of a frustum shows two concentric circles whichever end
 * faces the viewer. Printing a symbol that might be the wrong one round is
 * worse than printing none, so the row prints `Third angle` / `First angle`,
 * which ASME Y14.3 allows as a note and which cannot be misread. The glyph
 * is an open item, for whoever has the standard to hand.
 *
 * @param {any} sheet
 * @param {[number, number]} extent `[width, height]` in paper mm
 * @param {import('./style.js').DrawingStyle} style
 * @returns {{ svg: string, warnings: string[] }}
 */
export function renderTitleBlock(sheet, extent, style) {
	const warnings = [];
	if (sheet?.title_block?.show === false) return { svg: '', warnings };
	const rows = sheet?.title_block_cache?.rows ?? [];
	if (rows.length === 0) {
		// A title block the engine has not filled. Named rather than drawn
		// empty: an empty frame in the corner of a sheet reads as a title
		// block whose fields are blank, which is a different statement.
		if (sheet?.title_block) {
			warnings.push('the title block has no filled rows and was not drawn');
		}
		return { svg: '', warnings };
	}
	const [widthMm, heightMm] = extent;
	const inner = widthMm - 2 * SHEET_MARGIN_MM;
	const blockW = Math.min(style.titleBlockWidth, inner);
	const rowH = style.titleBlockRowHeight;
	const blockH = rowH * rows.length;
	// Bottom-right of the frame, which is where every standard puts it.
	const x = widthMm - SHEET_MARGIN_MM - blockW;
	const y = heightMm - SHEET_MARGIN_MM - blockH;
	if (![x, y, blockW, blockH].every((v) => Number.isFinite(v) && v > -widthMm)) {
		return { svg: '', warnings: ['the title block does not fit this sheet'] };
	}
	const labelW = Math.min(style.titleBlockLabelWidth, blockW / 2);
	const stroke = `stroke="${DRAWING_TOKENS.frame}" stroke-width="${n(style.thinWidth)}"`;
	const parts = [
		`<rect class="wi-title-frame" x="${n(x)}" y="${n(y)}" width="${n(blockW)}" height="${n(blockH)}" ` +
			`fill="none" ${stroke} />`
	];
	rows.forEach((row, i) => {
		const top = y + i * rowH;
		if (i > 0) {
			parts.push(
				`<line class="wi-title-rule" x1="${n(x)}" y1="${n(top)}" x2="${n(x + blockW)}" y2="${n(top)}" ${stroke} />`
			);
		}
		parts.push(
			`<line class="wi-title-rule" x1="${n(x + labelW)}" y1="${n(top)}" x2="${n(x + labelW)}" y2="${n(top + rowH)}" ${stroke} />`
		);
		// The label is small caps-ish by size rather than by transform: a
		// `font-variant` is a text feature a plotter may not have, where a
		// size is geometry.
		const mid = top + rowH / 2;
		parts.push(
			`<text class="wi-title-label" x="${n(x + 2)}" y="${n(mid)}" ` +
				`font-size="${n(style.textHeight * 0.7)}" font-family="${esc(style.fontFamily)}" ` +
				`fill="${DRAWING_TOKENS.text}" text-anchor="start" dominant-baseline="middle">` +
				`${esc(row?.label ?? '')}</text>`
		);
		parts.push(
			`<text class="wi-title-value" x="${n(x + labelW + 2)}" y="${n(mid)}" ` +
				`font-size="${n(style.textHeight)}" font-family="${esc(style.fontFamily)}" ` +
				`fill="${DRAWING_TOKENS.text}" text-anchor="start" dominant-baseline="middle">` +
				`${esc(row?.value ?? '')}</text>`
		);
	});
	return {
		svg: `<g class="wi-title-block" data-rows="${rows.length}">${parts.join('')}</g>`,
		warnings
	};
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
