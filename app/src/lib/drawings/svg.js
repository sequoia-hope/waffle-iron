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

/**
 * One `LayoutCurve` as a PAPER-SPACE polyline: `{ points, closed, dot }`.
 *
 * The geometry half of [`curvePath`], split out at D4b so the PDF writer and
 * the hatch generator read the same points the SVG path is built from rather
 * than re-deriving them. Nothing here is a second source of truth: `curvePath`
 * is now a formatter over this, so a change to the sampling moves both.
 *
 * `dot` marks a line seen end-on — one point, which a path has to emit
 * specially (see `curvePath`). `null` for a curve kind this build does not
 * know, which the caller reports.
 */
export function curvePoints(curve, tf) {
	switch (curve?.type) {
		case 'Point':
			return { points: [tf.toPaper(curve.at)], closed: false, dot: true };
		case 'Line':
			return {
				points: [tf.toPaper(curve.start), tf.toPaper(curve.end)],
				closed: false,
				dot: false
			};
		case 'Circle':
			return conicPoints(
				curve.center,
				curve.radius,
				curve.radius,
				[1, 0],
				curve.start_angle,
				curve.end_angle,
				tf
			);
		case 'Ellipse':
			return conicPoints(
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
			if (pts.length === 0) return { points: [], closed: false, dot: false };
			return {
				points: pts.map((p) => tf.toPaper(p)),
				closed: !!curve.closed,
				dot: false
			};
		}
		default:
			return null;
	}
}

/** An SVG `d` for one `LayoutCurve`, in paper mm. */
function curvePath(curve, tf) {
	const got = curvePoints(curve, tf);
	if (!got || got.points.length === 0) return '';
	if (got.dot) {
		// A line seen end-on. A zero-length path renders nothing with a butt
		// cap, so it is emitted as a round dot-sized segment.
		const [p] = got.points;
		return `M ${n(p[0])} ${n(p[1])} l 0 0`;
	}
	return polylinePath(got.points, got.closed);
}

/** `M … L … [Z]` over paper-space points. */
function polylinePath(points, closed) {
	let d = `M ${n(points[0][0])} ${n(points[0][1])}`;
	for (let i = 1; i < points.length; i++) {
		d += ` L ${n(points[i][0])} ${n(points[i][1])}`;
	}
	return closed ? `${d} Z` : d;
}

/**
 * A circle or ellipse arc sampled on its own parameter, in paper space.
 *
 * Why sampled rather than `A` (elliptical arc) commands: an `A` needs the
 * ellipse's rotation and the large-arc/sweep flags derived from a
 * parameterization SVG does not share with `Curve2`'s, and the v-flip into
 * paper space reverses the sweep. Sampling at a fixed count per quadrant is
 * deterministic, correct for every parameter range including a full
 * revolution, and at 16 segments per quadrant the chord error on a 50 mm
 * radius is under 1 µm on paper — below what the rounding keeps.
 */
function conicPoints(center, rMajor, rMinor, majorAxis, t0, t1, tf) {
	const span = Math.abs(t1 - t0);
	const segments = Math.max(8, Math.ceil((span / (Math.PI / 2)) * 16));
	const [ax, ay] = majorAxis ?? [1, 0];
	const alen = Math.hypot(ax, ay) || 1;
	const u = [ax / alen, ay / alen];
	// perp((x, y)) = (−y, x), as `Curve2::Ellipse` documents.
	const w = [-u[1], u[0]];
	const points = [];
	for (let i = 0; i <= segments; i++) {
		const t = t0 + ((t1 - t0) * i) / segments;
		const c = Math.cos(t) * rMajor;
		const s = Math.sin(t) * rMinor;
		points.push(tf.toPaper([center[0] + c * u[0] + s * w[0], center[1] + c * u[1] + s * w[1]]));
	}
	// A closed conic: the last sample joins the first, so the outline has no
	// hairline gap at the seam.
	return { points, closed: Math.abs(span - 2 * Math.PI) < 1e-9, dot: false };
}

/** A circular arc in PAPER space already, as a path — the annotation arcs. */
function paperArcPath(center, radius, startDeg, endDeg) {
	const got = conicPoints([0, 0], radius, radius, [1, 0], (startDeg * Math.PI) / 180, (endDeg * Math.PI) / 180, {
		mmPerMeter: 1,
		toPaper: ([x, y]) => [x + center[0], y + center[1]],
		dirToPaper: (d) => d
	});
	return polylinePath(got.points, got.closed);
}

// ─────────────────────────────────────────────── D4b: hatch, marks, crop

/**
 * A section cap's hatching, as PAPER-SPACE line segments
 * (`specs/drawings_and_mbd.md` §8, D4b).
 *
 * ## Why segments and not a pattern or a clip path
 *
 * An SVG `<pattern>` fill, or a `<clipPath>` with a family of long lines
 * through it, would both be shorter — and neither survives the trip to PDF
 * (and to DXF, whose `HATCH` entity is a different thing again). Computing
 * the segments means the hatch is the same geometry in every output, and the
 * line a reader measures on the screen is the line in the file.
 *
 * ## The fill rule
 *
 * Even-odd, by a scanline. The loops are rotated so the hatch direction
 * becomes horizontal, each scanline's crossings with every loop edge are
 * collected and sorted, and consecutive pairs are the inside. A crossing is
 * counted with the half-open test `(y0 <= y) !== (y1 <= y)`, which is what
 * makes a scanline passing exactly through a VERTEX count once rather than
 * twice — the classic even-odd bug, and the one a cap with a hole in it would
 * hit at the hole's extremes.
 *
 * Even-odd rather than the loops' own winding because a cap's loops already
 * carry their direction as `hole`, and an outer loop nested inside another
 * outer loop (two bodies, one inside a hollow of the other) is hatched
 * correctly by even-odd without anyone having to work out the nesting.
 *
 * @param {any[]} loops `HatchLoop`s: `{ curves, hole }`
 * @param {{ toPaper: (p: number[]) => number[] }} tf
 * @param {{ spacing: number, angleDeg: number }} options
 * @returns {{ segments: number[][][], warnings: string[] }}
 */
export function hatchSegments(loops, tf, { spacing, angleDeg }) {
	const warnings = [];
	const segments = [];
	if (!(spacing > 0) || !Number.isFinite(spacing)) {
		return { segments, warnings: ['the hatch spacing is not a positive length'] };
	}
	const theta = (angleDeg * Math.PI) / 180;
	const [c, s] = [Math.cos(theta), Math.sin(theta)];
	// Rotate BY −θ so the hatch lines lie along the x axis.
	const into = ([x, y]) => [x * c + y * s, -x * s + y * c];
	const back = ([x, y]) => [x * c - y * s, x * s + y * c];

	/** @type {number[][][]} */
	const rings = [];
	for (const loop of loops ?? []) {
		const ring = [];
		for (const curve of loop?.curves ?? []) {
			const got = curvePoints(curve, tf);
			if (!got || got.points.length === 0) {
				warnings.push(`a hatch boundary curve of type ${curve?.type ?? '?'} produced no points`);
				continue;
			}
			for (const p of got.points) {
				const q = into(p);
				// Consecutive curves of a loop SHARE an endpoint, so the join
				// would otherwise be a duplicate vertex — and a duplicate
				// vertex is a zero-length edge, which the crossing test counts
				// as neither in nor out.
				const last = ring[ring.length - 1];
				if (last && Math.abs(last[0] - q[0]) < 1e-12 && Math.abs(last[1] - q[1]) < 1e-12) {
					continue;
				}
				ring.push(q);
			}
		}
		if (ring.length >= 3) rings.push(ring);
		else if (ring.length > 0) {
			warnings.push('a hatch boundary loop had fewer than three distinct points');
		}
	}
	if (rings.length === 0) return { segments, warnings };

	let minY = Infinity;
	let maxY = -Infinity;
	for (const ring of rings) {
		for (const [, y] of ring) {
			if (!Number.isFinite(y)) {
				return { segments, warnings: [...warnings, 'a hatch boundary point is not finite'] };
			}
			minY = Math.min(minY, y);
			maxY = Math.max(maxY, y);
		}
	}
	// The scanlines are laid on a GLOBAL grid (multiples of the spacing from
	// the paper origin) rather than from the cap's own minimum, so two caps on
	// one sheet — a section of two bodies — carry one continuous hatch
	// pattern instead of two that nearly line up.
	const first = Math.ceil(minY / spacing) * spacing;
	// A cap larger than this many lines is a degenerate input (a spacing of
	// nothing, a cap the size of a building); reported rather than hung on.
	const MAX_LINES = 20000;
	let lines = 0;
	for (let y = first; y <= maxY; y += spacing) {
		if (++lines > MAX_LINES) {
			warnings.push(`the hatch stopped at ${MAX_LINES} lines; the cap is too large for the spacing`);
			break;
		}
		/** @type {number[]} */
		const crossings = [];
		for (const ring of rings) {
			for (let i = 0; i < ring.length; i++) {
				const [x0, y0] = ring[i];
				const [x1, y1] = ring[(i + 1) % ring.length];
				if (y0 <= y !== y1 <= y) {
					crossings.push(x0 + ((y - y0) / (y1 - y0)) * (x1 - x0));
				}
			}
		}
		crossings.sort((a, b) => a - b);
		for (let i = 0; i + 1 < crossings.length; i += 2) {
			if (crossings[i + 1] - crossings[i] <= 1e-9) continue;
			segments.push([back([crossings[i], y]), back([crossings[i + 1], y])]);
		}
	}
	return { segments, warnings };
}

/** The hatch as SVG, or `''`. */
function renderHatch(loops, tf, style) {
	const { segments, warnings } = hatchSegments(loops, tf, {
		spacing: style.hatchSpacing,
		angleDeg: style.hatchAngleDeg
	});
	if (segments.length === 0) return { svg: '', warnings };
	const lines = segments
		.map(
			([a, b]) =>
				`<line class="wi-hatch" x1="${n(a[0])}" y1="${n(a[1])}" x2="${n(b[0])}" y2="${n(b[1])}" ` +
				`stroke="${DRAWING_TOKENS.hatch}" stroke-width="${n(style.thinWidth)}" />`
		)
		.join('');
	return { svg: `<g class="wi-hatches" data-hatch-lines="${segments.length}">${lines}</g>`, warnings };
}

/**
 * The marks a view carries because other views derive from it (D4b): a child
 * section's cutting line with its arrows and letter, a child detail's circle
 * with its letter.
 *
 * Drawn on the PARENT, which is where the engine put them — a cutting line
 * belongs to the view it cuts.
 */
function renderMarks(marks, tf, style) {
	const warnings = [];
	const out = [];
	for (const mark of marks ?? []) {
		if (mark?.type === 'Section') {
			const a = tf.toPaper(mark.from);
			const b = tf.toPaper(mark.to);
			// The sight direction is a DIRECTION, so it flips with the paper's
			// y the same way a vector does — `dirToPaper`, not `toPaper`,
			// which would add the origin offset and point the arrows at the
			// corner of the sheet.
			const sight = tf.dirToPaper(mark.sight ?? [0, -1]);
			const slen = Math.hypot(sight[0], sight[1]) || 1;
			const dir = [sight[0] / slen, sight[1] / slen];
			if (![...a, ...b, ...dir].every(Number.isFinite)) {
				warnings.push('a section mark has a non-finite point');
				continue;
			}
			out.push(
				`<line class="wi-mark-cut" x1="${n(a[0])}" y1="${n(a[1])}" x2="${n(b[0])}" y2="${n(b[1])}" ` +
					`stroke="${DRAWING_TOKENS.annotation}" stroke-width="${n(style.cutLineWidth)}" ` +
					`stroke-dasharray="${esc(style.centreDash.join(' '))}" />`
			);
			for (const at of [a, b]) {
				out.push(arrowAt(at, dir, style));
				// The letter sits BEHIND the arrow (against the sight
				// direction), which is where the standard puts it: outside the
				// part, not over the view.
				const label = [
					at[0] - dir[0] * style.arrowLength * 2.2,
					at[1] - dir[1] * style.arrowLength * 2.2
				];
				out.push(
					`<text class="wi-mark-label" x="${n(label[0])}" y="${n(label[1])}" ` +
						`font-size="${n(style.textHeight)}" font-family="${esc(style.fontFamily)}" ` +
						`fill="${DRAWING_TOKENS.text}" text-anchor="middle" dominant-baseline="middle">` +
						`${esc(mark.label ?? '')}</text>`
				);
			}
		} else if (mark?.type === 'Detail') {
			const centre = tf.toPaper(mark.center);
			const r = Number(mark.radius) * tf.mmPerMeter;
			if (!Number.isFinite(r) || r <= 0 || !centre.every(Number.isFinite)) {
				warnings.push('a detail mark has no drawable circle');
				continue;
			}
			out.push(
				`<circle class="wi-mark-detail" cx="${n(centre[0])}" cy="${n(centre[1])}" r="${n(r)}" ` +
					`fill="none" stroke="${DRAWING_TOKENS.annotation}" stroke-width="${n(style.thinWidth)}" />`
			);
			out.push(
				`<text class="wi-mark-label" x="${n(centre[0] + r)}" y="${n(centre[1] - r)}" ` +
					`font-size="${n(style.textHeight)}" font-family="${esc(style.fontFamily)}" ` +
					`fill="${DRAWING_TOKENS.text}" text-anchor="start" dominant-baseline="auto">` +
					`${esc(mark.label ?? '')}</text>`
			);
		} else {
			// A mark kind this build does not know. Named rather than skipped:
			// a drawing silently missing a cutting line looks complete.
			warnings.push(`a view mark of type ${mark?.type ?? '?'} was not drawn`);
		}
	}
	return { svg: out.join(''), warnings };
}

/** A filled arrowhead at `at`, pointing along the unit `dir` (paper space). */
function arrowAt(at, dir, style) {
	const len = style.arrowLength;
	const half = (len * style.arrowWidthRatio) / 2;
	const perp = [-dir[1], dir[0]];
	const tip = [at[0] + dir[0] * len, at[1] + dir[1] * len];
	const base = [
		[at[0] + perp[0] * half, at[1] + perp[1] * half],
		[at[0] - perp[0] * half, at[1] - perp[1] * half]
	];
	const points = [tip, base[0], base[1]].map(([x, y]) => `${n(x)},${n(y)}`).join(' ');
	return `<polygon class="wi-mark-arrow" points="${points}" fill="${DRAWING_TOKENS.annotation}" />`;
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
			// The arc is already in paper space, so the transform is the
			// identity apart from the centre offset — and must NOT flip again.
			const d = paperArcPath(p.center, p.radius, p.startDeg, p.endDeg);
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
 * @param {string} [input.idPrefix] D4b: namespaces the `clipPath` id a detail
 *   view needs. A sheet nests several views in one document, so two details
 *   would otherwise declare the same id and the browser would clip both to
 *   whichever came first. The sheet passes the view's uuid.
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
	paper = true,
	idPrefix = 'v'
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

	// D4b. The cap's fill goes BEHIND the curves, so the cap's own boundary
	// edges stay the heaviest lines in the view; the marks a child view put
	// on this one go in front of both, because a cutting line crossing the
	// part has to be readable where it crosses.
	const hatch = renderHatch(layout?.hatch ?? [], tf, style);
	warnings.push(...hatch.warnings);
	const marks = renderMarks(layout?.marks ?? [], tf, style);
	warnings.push(...marks.warnings);

	// A detail view's crop: the renderer clips, which is what keeps the
	// detail's geometry analytic (the engine culls to the disc's box and
	// leaves the trimming here — see `ClipCircle`). The boundary circle is
	// drawn too: ISO 128-30 draws a detail's edge, and without it a crop
	// looks like a view whose part happens to end mid-air.
	const crop = layout?.clip ?? null;
	const cropCentre = crop ? tf.toPaper(crop.center) : null;
	const cropR = crop ? Number(crop.radius) * tf.mmPerMeter : 0;
	const cropOk = !!crop && Number.isFinite(cropR) && cropR > 0 && cropCentre.every(Number.isFinite);
	if (crop && !cropOk) warnings.push("a detail view's crop circle is not drawable");
	const clipId = `wi-crop-${String(idPrefix).replace(/[^A-Za-z0-9_-]/g, '')}`;
	const defs = cropOk
		? `<defs><clipPath id="${esc(clipId)}" clipPathUnits="userSpaceOnUse">` +
			`<circle cx="${n(cropCentre[0])}" cy="${n(cropCentre[1])}" r="${n(cropR)}" />` +
			`</clipPath></defs>`
		: '';
	const clipAttr = cropOk ? ` clip-path="url(#${esc(clipId)})"` : '';
	const cropEdge = cropOk
		? `<circle class="wi-crop-edge" cx="${n(cropCentre[0])}" cy="${n(cropCentre[1])}" r="${n(cropR)}" ` +
			`fill="none" stroke="${DRAWING_TOKENS.visible}" stroke-width="${n(style.thinWidth)}" />`
		: '';

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
		defs +
		(paper
			? `<rect class="wi-paper" x="${n(-pad)}" y="${n(-pad)}" width="${n(widthMm)}" height="${n(heightMm)}" fill="${DRAWING_TOKENS.paper}" />`
			: '') +
		`<g class="wi-section"${clipAttr}>${hatch.svg}</g>` +
		`<g class="wi-curves"${clipAttr}>${curveEls.join('')}</g>` +
		cropEdge +
		`<g class="wi-annotations">${annEls.join('')}</g>` +
		`<g class="wi-marks">${marks.svg}</g>` +
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
