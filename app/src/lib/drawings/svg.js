/**
 * The SVG dimension renderer (`specs/drawings_and_mbd.md` §7, increment D3).
 *
 * One exported function, [`renderViewSvg`]: a `ViewLayout` (the record
 * `waffle_types::annotation::layout` emits — projected curves plus resolved,
 * measured annotations) and a style in, one SVG document string out.
 *
 * ## It is a pure function, and that is load-bearing
 *
 * No DOM, no `Date`, no `Math.random`, no reads of module or component state.
 * Same inputs ⇒ byte-identical output, which buys three things:
 *
 * 1. `export_svg(tab)` (D4a) can write exactly what the sheet component
 *    shows, because they are the same string.
 * 2. A V3-style byte oracle can hash the output instead of comparing pixels.
 * 3. A Playwright spec can assert on the SVG DOM — element counts, the text
 *    of a value — without a screenshot baseline to rot.
 *
 * Every emitted coordinate goes through [`n`], which rounds to a fixed number
 * of decimals. Without it the output would carry full `f64` noise and differ
 * between machines on the last bit; with it, the string is stable and still
 * far finer than any plotter.
 *
 * ## One SVG user unit is one paper millimetre
 *
 * `layout.js` works in paper mm and the `viewBox` is in paper mm, so a 0.5 mm
 * line is `stroke-width="0.5"` and prints 0.5 mm wide at any view scale.
 * `width`/`height` are given in `mm` so the browser's print path produces a
 * true-to-scale sheet.
 *
 * ## Colours
 *
 * Every paint is a `var(--drawing-…)` reference (see `style.js`), never a
 * literal. The tokens are defined in `app/src/app.css` in terms of the
 * theme's own, so a drawing follows the document theme with no per-theme
 * drawing block.
 */

import { DRAWING_TOKENS, drawingStyle } from './style.js';
import { formatDimension, isKnownUnit } from './format.js';
import { layoutAnnotation, paperTransform } from './layout.js';

/** Decimals kept on every emitted coordinate. 0.1 µm on paper — plenty. */
export const COORD_DECIMALS = 4;

/**
 * A coordinate, rounded and normalized for the output string.
 *
 * `-0` would print as `"-0"`, which is the same point under a different name
 * and so a spurious diff in a byte oracle; `NaN`/`Infinity` would print as
 * `"NaN"` and silently produce an unrenderable path, so they become `0` and
 * are reported through [`renderViewSvg`]'s `warnings` instead of poisoning
 * the geometry.
 */
export function n(x) {
	if (!Number.isFinite(x)) return '0';
	const r = Number(x.toFixed(COORD_DECIMALS));
	return String(r === 0 ? 0 : r);
}

/**
 * XML-escape text content and attribute values.
 *
 * Every interpolated value goes through this, including the ones that come
 * from the STYLE rather than from the model (`fontFamily`, the dash patterns).
 * The style is the document-settings seam, so from D4a its strings are
 * document data; and the output is handed to `{@html}`, where an unescaped
 * `"` would close an attribute and let the rest be read as markup.
 */
export function esc(s) {
	return String(s ?? '')
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;');
}

/** How each curve kind/visibility pair is painted. */
function curveStroke(entry, style) {
	const hidden = entry.visibility === 'Hidden';
	if (hidden) {
		return {
			stroke: DRAWING_TOKENS.hidden,
			width: style.hiddenWidth,
			dash: style.hiddenDash.join(' ')
		};
	}
	if (entry.kind === 'SectionOutline') {
		return { stroke: DRAWING_TOKENS.section, width: style.visibleWidth, dash: null };
	}
	return { stroke: DRAWING_TOKENS.visible, width: style.visibleWidth, dash: null };
}

/** An SVG `d` for one `LayoutCurve`, in paper mm. */
function curvePath(curve, tf) {
	switch (curve?.type) {
		case 'Point': {
			// A line seen end-on. A zero-length path renders nothing with a
			// butt cap, so it is emitted as a round dot-sized segment.
			const p = tf.toPaper(curve.at);
			return `M ${n(p[0])} ${n(p[1])} l 0 0`;
		}
		case 'Line': {
			const a = tf.toPaper(curve.start);
			const b = tf.toPaper(curve.end);
			return `M ${n(a[0])} ${n(a[1])} L ${n(b[0])} ${n(b[1])}`;
		}
		case 'Circle':
			return conicPath(
				curve.center,
				curve.radius,
				curve.radius,
				[1, 0],
				curve.start_angle,
				curve.end_angle,
				tf
			);
		case 'Ellipse':
			return conicPath(
				curve.center,
				curve.major_radius,
				curve.minor_radius,
				curve.major_axis,
				curve.start_param,
				curve.end_param,
				tf
			);
		case 'Polyline': {
			const pts = curve.points ?? [];
			if (pts.length === 0) return '';
			const head = tf.toPaper(pts[0]);
			let d = `M ${n(head[0])} ${n(head[1])}`;
			for (let i = 1; i < pts.length; i++) {
				const p = tf.toPaper(pts[i]);
				d += ` L ${n(p[0])} ${n(p[1])}`;
			}
			return curve.closed ? `${d} Z` : d;
		}
		default:
			return '';
	}
}

/**
 * A circle or ellipse arc as a polyline-free SVG path, sampled on its own
 * parameter.
 *
 * Why sampled rather than `A` (elliptical arc) commands: an `A` needs the
 * ellipse's rotation and the large-arc/sweep flags derived from a
 * parameterization SVG does not share with `Curve2`'s, and the v-flip into
 * paper space reverses the sweep. Sampling at a fixed count per quadrant is
 * deterministic, correct for every parameter range including a full
 * revolution, and at 16 segments per quadrant the chord error on a 50 mm
 * radius is under 1 µm on paper — below what the rounding keeps.
 */
function conicPath(center, rMajor, rMinor, majorAxis, t0, t1, tf) {
	const span = Math.abs(t1 - t0);
	const segments = Math.max(8, Math.ceil((span / (Math.PI / 2)) * 16));
	const [ax, ay] = majorAxis ?? [1, 0];
	const alen = Math.hypot(ax, ay) || 1;
	const u = [ax / alen, ay / alen];
	// perp((x, y)) = (−y, x), as `Curve2::Ellipse` documents.
	const w = [-u[1], u[0]];
	let d = '';
	for (let i = 0; i <= segments; i++) {
		const t = t0 + ((t1 - t0) * i) / segments;
		const c = Math.cos(t) * rMajor;
		const s = Math.sin(t) * rMinor;
		const p = tf.toPaper([center[0] + c * u[0] + s * w[0], center[1] + c * u[1] + s * w[1]]);
		d += `${i === 0 ? 'M' : ' L'} ${n(p[0])} ${n(p[1])}`;
	}
	// A closed conic: join the last sample back to the first so the outline
	// has no hairline gap at the seam.
	if (Math.abs(span - 2 * Math.PI) < 1e-9) d += ' Z';
	return d;
}

/** Paint for an annotation primitive, by its role. */
function primitiveStroke(role, style) {
	const thin = { stroke: DRAWING_TOKENS.annotation, width: style.thinWidth, dash: null };
	if (role === 'centre') return { ...thin, dash: style.centreDash.join(' ') };
	return thin;
}

function renderPrimitive(p, style) {
	switch (p.kind) {
		case 'line': {
			const s = primitiveStroke(p.role, style);
			const dash = s.dash ? ` stroke-dasharray="${esc(s.dash)}"` : '';
			return `<line class="wi-dim-${esc(p.role)}" x1="${n(p.from[0])}" y1="${n(p.from[1])}" x2="${n(p.to[0])}" y2="${n(p.to[1])}" stroke="${s.stroke}" stroke-width="${n(s.width)}"${dash} />`;
		}
		case 'arc': {
			const s = primitiveStroke(p.role, style);
			const d = conicPath(
				[0, 0],
				p.radius,
				p.radius,
				[1, 0],
				(p.startDeg * Math.PI) / 180,
				(p.endDeg * Math.PI) / 180,
				// The arc is already in paper space, so the transform is the
				// identity apart from the centre offset — and must NOT flip
				// again.
				{
					mmPerMeter: 1,
					toPaper: ([x, y]) => [x + p.center[0], y + p.center[1]],
					dirToPaper: (dd) => dd
				}
			);
			return `<path class="wi-dim-${esc(p.role)}" d="${d}" fill="none" stroke="${s.stroke}" stroke-width="${n(s.width)}" />`;
		}
		case 'polygon': {
			const pts = p.points.map(([x, y]) => `${n(x)},${n(y)}`).join(' ');
			return `<polygon class="wi-dim-${esc(p.role)}" points="${pts}" fill="${DRAWING_TOKENS.annotation}" />`;
		}
		case 'dot':
			return `<circle class="wi-dim-${esc(p.role)}" cx="${n(p.at[0])}" cy="${n(p.at[1])}" r="${n(p.radius)}" fill="${DRAWING_TOKENS.annotation}" />`;
		case 'box': {
			const s = primitiveStroke(p.role, style);
			return `<rect class="wi-dim-${esc(p.role)}" x="${n(p.at[0])}" y="${n(p.at[1])}" width="${n(p.width)}" height="${n(p.height)}" fill="none" stroke="${s.stroke}" stroke-width="${n(s.width)}" />`;
		}
		case 'text': {
			const rot =
				p.rotateDeg === 0
					? ''
					: ` transform="rotate(${n(p.rotateDeg)} ${n(p.at[0])} ${n(p.at[1])})"`;
			return `<text class="wi-dim-${esc(p.role)}" x="${n(p.at[0])}" y="${n(p.at[1])}" font-size="${n(style.textHeight)}" font-family="${esc(style.fontFamily)}" fill="${DRAWING_TOKENS.text}" text-anchor="${esc(p.anchor)}" dominant-baseline="${esc(p.baseline)}"${rot}>${esc(p.text)}</text>`;
		}
		default:
			return '';
	}
}

/** The view-space bounding box of a layout, or `null`. */
function viewBounds(layout) {
	if (layout?.bbox) return layout.bbox;
	return null;
}

/**
 * Render one annotated view as a standalone SVG document.
 *
 * @param {object} input
 * @param {any} input.layout  a `ViewLayout` (curves, bbox, annotations)
 * @param {number} [input.scale] drawing scale as a ratio; 1 = 1:1
 * @param {Partial<import('./style.js').DrawingStyle>} [input.style]
 * @param {string} [input.unit] display unit for dimension text
 * @param {number} [input.documentPrecision]
 * @param {number} [input.margin] paper mm of blank around the drawing
 * @param {string|null} [input.title] an `<title>` for accessibility
 * @param {boolean} [input.paper] paint the paper rectangle behind the drawing
 *   (default true). A view composed onto a SHEET (D4a) passes false: the sheet
 *   paints one piece of paper, and a rectangle per view would read as a stack
 *   of cards rather than as one drawing.
 * @returns {{ svg: string, widthMm: number, heightMm: number, warnings: string[] }}
 */
export function renderViewSvg({
	layout,
	scale = 1,
	style: styleOverrides,
	unit = 'mm',
	documentPrecision = 2,
	margin = 20,
	title = null,
	paper = true
}) {
	const style = drawingStyle(styleOverrides);
	const warnings = [];
	const curves = layout?.curves ?? [];
	const annotations = layout?.annotations ?? [];

	const bounds = viewBounds(layout);
	// With no curves there is no view extent to lay a sheet out against. An
	// annotation-only layout still gets a sheet, centred on the origin, so a
	// note on an empty view is visible rather than silently dropped.
	const [[minU, minV], [maxU, maxV]] = bounds ?? [
		[0, 0],
		[0, 0]
	];
	const tf = paperTransform({ scale, origin: [minU, maxV] });
	const drawnW = (maxU - minU) * tf.mmPerMeter;
	const drawnH = (maxV - minV) * tf.mmPerMeter;

	const centrePaper = /** @type {[number, number]} */ ([drawnW / 2, drawnH / 2]);
	// The view's own box in paper space: the origin is its top-left corner by
	// construction, so it spans (0, 0) to (drawnW, drawnH). A linear dimension
	// has to clear this, not just its own witness points.
	const boundsPaper = bounds
		? /** @type {[number, number][]} */ ([
				[0, 0],
				[drawnW, drawnH]
			])
		: undefined;

	const curveEls = [];
	for (const entry of curves) {
		// A non-finite coordinate becomes `0` in the output (see `n`), which
		// would quietly move a curve to the origin. The annotation primitives
		// already have this tripwire; the curves need it too.
		for (const bad of nonFiniteIn(entry?.geometry ?? {})) {
			warnings.push(`a ${entry?.geometry?.type ?? '?'} curve had a non-finite ${bad}`);
		}
		const d = curvePath(entry?.geometry, tf);
		if (!d) {
			warnings.push(`a curve of kind ${entry?.geometry?.type ?? '?'} produced no path`);
			continue;
		}
		const s = curveStroke(entry, style);
		const dash = s.dash ? ` stroke-dasharray="${esc(s.dash)}"` : '';
		const cls = `wi-curve wi-curve-${String(entry.kind).toLowerCase()} wi-curve-${String(entry.visibility).toLowerCase()}`;
		curveEls.push(
			`<path class="${cls}" d="${d}" fill="none" stroke="${s.stroke}" stroke-width="${n(s.width)}" stroke-linecap="round"${dash} />`
		);
	}

	// A unit key `units.js` does not know converts by a factor of 1 — it would
	// print a 40 mm feature as "0.04" and label it with whatever was asked
	// for. `formatDimension` withholds the number instead; say why, because a
	// dash with no explanation is a bug report waiting to happen.
	if (!isKnownUnit(unit)) {
		warnings.push(`unknown display unit "${unit}" — dimension values withheld`);
	}

	const annEls = [];
	for (const a of annotations) {
		if (a?.dual_unit && !isKnownUnit(a.dual_unit)) {
			warnings.push(`unknown dual unit "${a.dual_unit}" — omitted`);
		}
		const text = annotationText(a, { unit, documentPrecision });
		const primitives = layoutAnnotation({ ...a, text }, style, tf, centrePaper, boundsPaper);
		if (primitives.length === 0) {
			warnings.push(
				`an annotation of type ${a?.type ?? '?'}${a?.kind?.type ? `/${a.kind.type}` : ''} laid out nothing`
			);
			continue;
		}
		for (const p of primitives) {
			for (const bad of nonFiniteIn(p)) {
				warnings.push(`${a?.type}: a ${p.kind} primitive had a non-finite ${bad}`);
			}
			annEls.push(renderPrimitive(p, style));
		}
	}

	// The drawing plus its margin. Annotations legitimately sit outside the
	// part's own box (that is what a dimension offset IS), so the margin has
	// to be at least the dimension offset plus a text height, or the leftmost
	// dimension is clipped off the sheet.
	const pad = Math.max(margin, style.dimensionOffset + style.textHeight * 2);
	const widthMm = round4(drawnW + 2 * pad);
	const heightMm = round4(drawnH + 2 * pad);
	const titleEl = title ? `<title>${esc(title)}</title>` : '';

	const svg =
		`<svg xmlns="http://www.w3.org/2000/svg" class="wi-drawing" ` +
		`width="${n(widthMm)}mm" height="${n(heightMm)}mm" ` +
		`viewBox="${n(-pad)} ${n(-pad)} ${n(widthMm)} ${n(heightMm)}" ` +
		`data-scale="${n(scale)}" data-curves="${curves.length}" data-annotations="${annotations.length}">` +
		titleEl +
		(paper
			? `<rect class="wi-paper" x="${n(-pad)}" y="${n(-pad)}" width="${n(widthMm)}" height="${n(heightMm)}" fill="${DRAWING_TOKENS.paper}" />`
			: '') +
		`<g class="wi-curves">${curveEls.join('')}</g>` +
		`<g class="wi-annotations">${annEls.join('')}</g>` +
		`</svg>`;

	return { svg, widthMm, heightMm, warnings };
}

/** The formatted text an annotation shows, or `''` where it shows none. */
export function annotationText(a, { unit = 'mm', documentPrecision = 2 } = {}) {
	switch (a?.type) {
		case 'Dimension':
			return formatDimension({
				value: a.value,
				kind: a.kind,
				unit,
				precision: a.precision ?? null,
				documentPrecision,
				dualUnit: a.dual_unit ?? null
			});
		case 'Note':
			return a.text ?? '';
		case 'Datum':
			return a.label ?? '';
		default:
			return '';
	}
}

/** The names of any non-finite numbers in a primitive — a NaN tripwire. */
function nonFiniteIn(p) {
	const bad = [];
	const check = (name, v) => {
		if (typeof v === 'number' && !Number.isFinite(v)) bad.push(name);
	};
	for (const [key, v] of Object.entries(p)) {
		if (Array.isArray(v)) {
			v.flat().forEach((x, i) => check(`${key}[${i}]`, x));
		} else {
			check(key, v);
		}
	}
	return bad;
}

function round4(x) {
	return Number.isFinite(x) ? Number(x.toFixed(COORD_DECIMALS)) : 0;
}
