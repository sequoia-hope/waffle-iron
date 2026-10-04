/**
 * Dimension layout — where the extension lines, dimension line, arrowheads
 * and text GO (`specs/drawings_and_mbd.md` §7's render list).
 *
 * This module is pure 2-D geometry over the `AnnotationLayout` records
 * `waffle_types::annotation::layout` emits. It produces drawing primitives;
 * `svg.js` turns those into markup. The split exists so the placement rules
 * can be reasoned about and tested without any string handling, and so the
 * 3-D PMI overlay (M2) can reuse them against a different emitter.
 *
 * ## Two coordinate systems, and the one place they meet
 *
 * - **Model/view space**: the view's own `(u, v)`, in METERS, `v` up. This is
 *   what the kernel's projection and every anchor are in.
 * - **Paper space**: millimetres on the sheet, `y` DOWN, which is what SVG
 *   wants and what a plotter measures.
 *
 * Everything an annotation derives from geometry (witness points, a circle's
 * centre, two lines' intersection) happens in view space; everything the
 * standards specify (a 2 mm gap, a 3.5 mm arrowhead, a 10 mm clearance)
 * happens in paper space. So the layout converts ONCE, at the top of
 * `layoutAnnotation`, via [`paperTransform`] — and from there on works in
 * paper mm. Mixing the two is how a dimension ends up with arrowheads that
 * grow when you scale the view.
 *
 * The flip is applied to the coordinates, NOT as an SVG transform: a
 * `scale(1, -1)` would mirror the text too.
 *
 * ## Determinism
 *
 * No `Date`, no `Math.random`, no iteration over object keys whose order is
 * not fixed by the input. Same `ViewLayout` and same style in, identical
 * primitives out — which is what makes a byte oracle over the SVG possible.
 */

/**
 * @typedef {{ kind: 'line', from: [number, number], to: [number, number],
 *             role: string }} LineP
 * @typedef {{ kind: 'arc', center: [number, number], radius: number,
 *             startDeg: number, endDeg: number, role: string }} ArcP
 * @typedef {{ kind: 'polygon', points: [number, number][], role: string }} PolygonP
 * @typedef {{ kind: 'dot', at: [number, number], radius: number, role: string }} DotP
 * @typedef {{ kind: 'text', at: [number, number], text: string,
 *             anchor: 'start' | 'middle' | 'end',
 *             baseline: 'auto' | 'middle' | 'hanging',
 *             rotateDeg: number, role: string }} TextP
 * @typedef {{ kind: 'box', at: [number, number], width: number, height: number,
 *             role: string, rotateDeg?: number }} BoxP
 * @typedef {LineP | ArcP | PolygonP | DotP | TextP | BoxP} Primitive
 */

/**
 * Line pitch as a multiple of the text height, for the stacked forms (M1).
 * ISO 3098 sets the minimum line spacing of lettering at 1.4 × the character
 * height, which is what a stacked limit pair needs to stay legible.
 */
export const LINE_SPACING = 1.4;

/**
 * The nominal advance width of `text` at `textHeight`, in paper mm.
 *
 * 0.6 em per character — above Helvetica's digit advance (0.556 em) and above
 * every glyph a dimension uses except a few letters, so a box sized from this
 * never clips the number it encloses.
 *
 * An ESTIMATE on purpose. `layout.js` is a pure function with no DOM, and §3
 * of the spec forbids text metrics in Rust, so the only honest source for a
 * box's width is the style's own text height. A box 10 % too wide is a
 * cosmetic matter; a measured width that needs the DOM would make the
 * renderer unusable as a byte oracle.
 *
 * @param {string} text
 * @param {number} textHeight paper mm
 */
export function textWidthMm(text, textHeight) {
	return String(text ?? '').length * textHeight * 0.6;
}

/**
 * The view-space → paper-space map for a view.
 *
 * `scale` is the drawing scale as a ratio (1 = 1:1, 0.5 = 1:2). The model is
 * in meters, the sheet in millimetres, so a meter becomes `1000 · scale`
 * paper mm.
 *
 * @param {{ scale?: number, origin?: [number, number] }} [opts]
 *   `origin` is the view-space point that lands on paper `(0, 0)`; default
 *   `[0, 0]`.
 * @returns {{ mmPerMeter: number, toPaper: (p: [number, number]) => [number, number],
 *             dirToPaper: (d: [number, number]) => [number, number] }}
 */
export function paperTransform(opts) {
	const scale = opts?.scale ?? 1;
	const [ou, ov] = opts?.origin ?? [0, 0];
	const mmPerMeter = 1000 * scale;
	return {
		mmPerMeter,
		toPaper: ([u, v]) => [(u - ou) * mmPerMeter, -(v - ov) * mmPerMeter],
		// A direction only flips in v; it is not translated, and it is NOT
		// scaled — callers want unit directions in paper space.
		dirToPaper: ([du, dv]) => {
			const [x, y] = [du, -dv];
			const len = Math.hypot(x, y);
			return len === 0 ? [0, 0] : [x / len, y / len];
		}
	};
}

/** @param {[number, number]} a @param {[number, number]} b */
const sub = (a, b) => /** @type {[number, number]} */ ([a[0] - b[0], a[1] - b[1]]);
/** @param {[number, number]} a @param {[number, number]} b */
const add = (a, b) => /** @type {[number, number]} */ ([a[0] + b[0], a[1] + b[1]]);
/** @param {[number, number]} a @param {number} k */
const mul = (a, k) => /** @type {[number, number]} */ ([a[0] * k, a[1] * k]);
/** @param {[number, number]} a @param {[number, number]} b */
const dot = (a, b) => a[0] * b[0] + a[1] * b[1];
/** @param {[number, number]} a */
const norm = (a) => {
	const l = Math.hypot(a[0], a[1]);
	return l === 0 ? /** @type {[number, number]} */ ([0, 0]) : mul(a, 1 / l);
};
/** Rotate 90° counter-clockwise in paper space (y down ⇒ visually clockwise). */
const perp = (/** @type {[number, number]} */ a) =>
	/** @type {[number, number]} */ ([-a[1], a[0]]);

/**
 * The one point an anchor is measured from, in view space — the mirror of
 * `LayoutCurve::witness_point` in Rust. `null` where Rust returns `None`
 * (a sampled polyline has no canonical witness).
 *
 * @param {any} anchor an `AnchorGeometry`
 * @returns {[number, number] | null}
 */
export function witnessPoint(anchor) {
	if (!anchor) return null;
	if (anchor.type === 'Point') return anchor.at;
	const c = anchor.curve;
	if (!c) return null;
	switch (c.type) {
		case 'Point':
			return c.at;
		case 'Line':
			return [(c.start[0] + c.end[0]) / 2, (c.start[1] + c.end[1]) / 2];
		case 'Circle':
		case 'Ellipse':
			return c.center;
		default:
			return null;
	}
}

/** The unit direction of a line anchor in view space, or `null`. */
function anchorDirection(anchor) {
	const c = anchor?.curve;
	if (!c || c.type !== 'Line') return null;
	const d = sub(c.end, c.start);
	return Math.hypot(d[0], d[1]) === 0 ? null : norm(d);
}

/**
 * The direction a dimension of `kind` measures ALONG, in paper space.
 *
 * `HDistance` and `VDistance` are axis-locked by definition. An aligned
 * `Distance` runs along the line joining the two witness points — except
 * between two parallel lines, where it runs across them, because that is the
 * quantity `measure` returned and the drawn arrows must agree with the
 * printed number. `PointLineDistance` likewise runs perpendicular to the line.
 *
 * @returns {[number, number] | null} null when the anchors give no direction
 */
function measurementDirection(kind, anchors, tf) {
	const k = kind?.type;
	if (k === 'HDistance') return [1, 0];
	if (k === 'VDistance') return [0, 1];

	const d0 = anchorDirection(anchors[0]);
	const d1 = anchorDirection(anchors[1]);
	if (k === 'PointLineDistance') {
		return d1 ? perp(tf.dirToPaper(d1)) : null;
	}
	if (k === 'Distance') {
		// Two parallel lines: across them.
		if (d0 && d1 && Math.abs(d0[0] * d1[1] - d0[1] * d1[0]) <= 1e-7) {
			return perp(tf.dirToPaper(d0));
		}
		const p0 = witnessPoint(anchors[0]);
		const p1 = witnessPoint(anchors[1]);
		if (!p0 || !p1) return null;
		const d = sub(tf.toPaper(p1), tf.toPaper(p0));
		return Math.hypot(d[0], d[1]) === 0 ? null : norm(d);
	}
	return null;
}

/**
 * A linear dimension: two extension lines, the dimension line, two
 * arrowheads, the text.
 *
 * The dimension line sits `style.dimensionOffset` clear of the whole VIEW, on
 * the side away from the view's centre, plus the annotation's own `placement`.
 *
 * Clearing the view rather than the two witness points is what actually keeps
 * the line off the part, and the two differ: a witness point is a wall's
 * MIDPOINT, not the part's extreme. Two opposite walls dimensioned for width
 * have their midpoints at mid-height, so a 10 mm offset from them put the
 * dimension line 2.5 mm INSIDE a 40 × 25 mm plate — line, arrowheads and both
 * extension lines, which then ran backwards into the part. The view's own
 * bounding box is the only thing that knows where the part ends; without one
 * (an annotation-only layout) the witness points are all there is.
 *
 * `placement` moves the whole dimension, not only its label — the same
 * behaviour as dragging a sketch dimension, and the reason it can push a
 * dimension to the other side of the part when a drafter wants it there.
 */
function layoutLinear(a, style, tf, centrePaper, boundsPaper) {
	const p0 = witnessPoint(a.anchors?.[0]);
	const p1 = witnessPoint(a.anchors?.[1]);
	if (!p0 || !p1) return [];
	const q0 = tf.toPaper(p0);
	const q1 = tf.toPaper(p1);
	const d = measurementDirection(a.kind, a.anchors, tf);
	if (!d) return [];
	const n = perp(d);

	// Which side of the view centre these anchors are on, measured along n.
	const mid = mul(add(q0, q1), 0.5);
	const side = dot(mid, n) >= dot(centrePaper, n) ? 1 : -1;
	const clear = [q0, q1, ...boxCorners(boundsPaper)].map((q) => dot(q, n));
	const nFar = side > 0 ? Math.max(...clear) : Math.min(...clear);
	const placement = paperPlacement(a.placement, tf);
	const nLine = nFar + side * style.dimensionOffset + dot(placement, n);

	// Feet of the two witness points on the dimension line, slid along d by
	// the placement's own along-component so a dragged label carries the
	// witness feet with it.
	const alongShift = dot(placement, d);
	const foot = (q) => add(add(q, mul(n, nLine - dot(q, n))), mul(d, alongShift));
	const f0 = foot(q0);
	const f1 = foot(q1);

	const out = [];
	for (const [q, f] of [
		[q0, f0],
		[q1, f1]
	]) {
		const away = norm(sub(f, q));
		if (Math.hypot(away[0], away[1]) === 0) continue;
		out.push({
			kind: 'line',
			from: add(q, mul(away, style.extensionGap)),
			to: add(f, mul(away, style.extensionOvershoot)),
			role: 'extension'
		});
	}
	out.push({ kind: 'line', from: f0, to: f1, role: 'dimension' });
	// Heads point OUTWARD from the middle of the dimension line, so they sit
	// against the extension lines the way a drafter draws them.
	out.push(...arrowhead(f0, norm(sub(f0, f1)), style));
	out.push(...arrowhead(f1, norm(sub(f1, f0)), style));

	// Text above the dimension line (on the side away from the part), rotated
	// with it so it reads along the dimension — ISO 129-1's aligned method.
	const textAt = add(mul(add(f0, f1), 0.5), mul(n, side * (style.textGap + style.textHeight / 2)));
	out.push(
		...valueTexts(a, style, textAt, {
			anchor: 'middle',
			baseline: 'middle',
			rotateDeg: readableAngleDeg(d),
			away: mul(n, side)
		})
	);
	return out;
}

/**
 * A radial dimension: a leader from the centre out through the rim, an
 * arrowhead ON the rim pointing outward, a shoulder, and the text.
 */
function layoutRadial(a, style, tf, isDiameter) {
	const curve = a.anchors?.[0]?.curve;
	if (!curve || (curve.type !== 'Circle' && curve.type !== 'Ellipse')) return [];
	const centre = tf.toPaper(curve.center);
	const rPaper = (curve.type === 'Circle' ? curve.radius : curve.major_radius) * tf.mmPerMeter;
	if (!(rPaper > 0)) return [];

	// The leader direction: the placement if the user moved the label,
	// otherwise the style's default angle. Measured in PAPER space, where y
	// is down, so the default 45° points down-right — the quadrant a drafter
	// uses when nothing else is in the way.
	const placement = paperPlacement(a.placement, tf);
	const dir =
		Math.hypot(placement[0], placement[1]) > 0
			? norm(placement)
			: /** @type {[number, number]} */ ([
					Math.cos((style.leaderAngleDeg * Math.PI) / 180),
					Math.sin((style.leaderAngleDeg * Math.PI) / 180)
				]);

	// How far the RIM is along `dir`. For a circle that is the radius; for a
	// foreshortened hole it is not, and using the major radius there floated
	// the arrowhead off the ellipse it was pointing at — by up to
	// major − minor, 2.3 mm on a Ø16 rim seen at 45°. The printed value stays
	// the true (major) radius; only the arrow follows the drawn curve.
	const rimPaper = curve.type === 'Circle' ? rPaper : ellipseReach(curve, dir, tf);
	if (!(rimPaper > 0)) return [];

	const onRim = add(centre, mul(dir, rimPaper));
	const knee = add(centre, mul(dir, rimPaper + style.dimensionOffset));
	const shoulderSign = dir[0] >= 0 ? 1 : -1;
	const shoulderEnd = add(knee, [shoulderSign * style.leaderShoulder, 0]);

	const out = [];
	if (isDiameter) {
		// Through the centre: both rims get a head, and the line spans the
		// full diameter rather than starting at the centre. A conic is
		// centrally symmetric, so the opposite rim is the same reach back.
		const other = add(centre, mul(dir, -rimPaper));
		out.push({ kind: 'line', from: other, to: knee, role: 'dimension' });
		out.push(...arrowhead(other, mul(dir, -1), style));
	} else {
		out.push({ kind: 'line', from: centre, to: knee, role: 'dimension' });
	}
	out.push(...arrowhead(onRim, dir, style));
	out.push({ kind: 'line', from: knee, to: shoulderEnd, role: 'leader' });
	out.push(
		...valueTexts(a, style, add(shoulderEnd, [shoulderSign * style.textGap, -style.textGap]), {
			anchor: shoulderSign > 0 ? 'start' : 'end',
			baseline: 'auto',
			rotateDeg: 0
		})
	);
	return out;
}

/**
 * An angular dimension: an arc centred on the two edges' intersection, from
 * one edge to the other, arrowheads tangent at each end, text at the arc's
 * midpoint.
 *
 * With no intersection — two parallel edges, which `measure` reads as 0° —
 * there is no centre to swing an arc about, so nothing is drawn rather than
 * something misleading.
 */
function layoutAngular(a, style, tf) {
	const c0 = a.anchors?.[0]?.curve;
	const c1 = a.anchors?.[1]?.curve;
	if (c0?.type !== 'Line' || c1?.type !== 'Line') return [];
	const s0 = tf.toPaper(c0.start);
	const e0 = tf.toPaper(c0.end);
	const s1 = tf.toPaper(c1.start);
	const e1 = tf.toPaper(c1.end);
	const apex = lineIntersection(s0, e0, s1, e1);
	if (!apex) return [];

	// Swing the arc through the FAR end of each edge, so the arc lies in the
	// wedge the two edges actually bound.
	const far = (s, e) => (Math.hypot(...sub(e, apex)) >= Math.hypot(...sub(s, apex)) ? e : s);
	const r0 = sub(far(s0, e0), apex);
	const r1 = sub(far(s1, e1), apex);
	const radius = Math.min(Math.hypot(...r0), Math.hypot(...r1)) * 0.6 + style.dimensionOffset;
	if (!(radius > 0)) return [];

	let a0 = (Math.atan2(r0[1], r0[0]) * 180) / Math.PI;
	let a1 = (Math.atan2(r1[1], r1[0]) * 180) / Math.PI;
	// The arc of the wedge, which is the SHORTER of the two sweeps: the
	// dimension measures the acute angle between the edges.
	let sweep = a1 - a0;
	while (sweep <= -180) sweep += 360;
	while (sweep > 180) sweep -= 360;
	if (sweep < 0) {
		[a0, a1] = [a1, a0];
		sweep = -sweep;
	}

	const at = (deg) =>
		add(apex, [
			radius * Math.cos((deg * Math.PI) / 180),
			radius * Math.sin((deg * Math.PI) / 180)
		]);
	const out = [
		{ kind: 'arc', center: apex, radius, startDeg: a0, endDeg: a0 + sweep, role: 'dimension' }
	];
	// Tangent at each end, pointing along the arc out of the wedge.
	const tangent = (deg, sign) =>
		/** @type {[number, number]} */ ([
			-sign * Math.sin((deg * Math.PI) / 180),
			sign * Math.cos((deg * Math.PI) / 180)
		]);
	out.push(...arrowhead(at(a0), tangent(a0, -1), style));
	out.push(...arrowhead(at(a0 + sweep), tangent(a0 + sweep, 1), style));
	const midDeg = a0 + sweep / 2;
	const outward = /** @type {[number, number]} */ ([
		Math.cos((midDeg * Math.PI) / 180),
		Math.sin((midDeg * Math.PI) / 180)
	]);
	const textAt = add(apex, mul(outward, radius + style.textGap + style.textHeight / 2));
	out.push(
		...valueTexts(a, style, textAt, {
			anchor: 'middle',
			baseline: 'middle',
			rotateDeg: 0,
			// Away from the arc is radially outward, so a stacked pair that
			// would grow back onto the arc is pushed clear of it instead.
			away: outward
		})
	);
	return out;
}

/** An ordinate dimension: one extension line and the value at its end. */
function layoutOrdinate(a, style, tf) {
	const p = witnessPoint(a.anchors?.[0]);
	if (!p) return [];
	const q = tf.toPaper(p);
	const axisU = a.kind?.axis?.type === 'U';
	// Runs perpendicular to the axis it reads: a u-ordinate's witness line
	// goes up the sheet to a value above the part.
	const away = /** @type {[number, number]} */ (axisU ? [0, -1] : [1, 0]);
	const placement = paperPlacement(a.placement, tf);
	const end = add(add(q, mul(away, style.dimensionOffset)), placement);
	return [
		{
			kind: 'line',
			from: add(q, mul(away, style.extensionGap)),
			to: end,
			role: 'extension'
		},
		...valueTexts(a, style, add(end, mul(away, style.textGap)), {
			anchor: axisU ? 'middle' : 'start',
			baseline: axisU ? 'auto' : 'middle',
			rotateDeg: 0,
			away
		})
	];
}

/** A note: text, with an optional leader ending in a dot or an arrow. */
function layoutNote(a, style, tf) {
	const placement = paperPlacement(a.placement, tf);
	const anchorPoint = witnessPoint(a.leader);
	if (!anchorPoint) {
		// A free note sits where its placement puts it, with nothing to point
		// at. `[0, 0]` plus the placement is the only well-defined position.
		return [
			{
				kind: 'text',
				at: placement,
				text: a.text,
				anchor: 'start',
				baseline: 'auto',
				rotateDeg: 0,
				role: 'note'
			}
		];
	}
	const q = tf.toPaper(anchorPoint);
	const dir =
		Math.hypot(placement[0], placement[1]) > 0
			? norm(placement)
			: /** @type {[number, number]} */ ([
					Math.cos((style.leaderAngleDeg * Math.PI) / 180),
					-Math.sin((style.leaderAngleDeg * Math.PI) / 180)
				]);
	const knee = add(q, mul(dir, style.dimensionOffset));
	const shoulderSign = dir[0] >= 0 ? 1 : -1;
	const end = add(knee, [shoulderSign * style.leaderShoulder, 0]);
	const out = [
		{ kind: 'line', from: q, to: knee, role: 'leader' },
		{ kind: 'line', from: knee, to: end, role: 'leader' }
	];
	// A leader onto a FACE gets a dot, onto an edge or vertex an arrow
	// (ISO 128-22). The record tells us which by the anchor's own shape: a
	// bare point came from a vertex or a face centroid, a curve from an edge.
	if (a.leader?.type === 'Point') {
		out.push({ kind: 'dot', at: q, radius: style.dotRadius, role: 'leader' });
	} else {
		out.push(...arrowhead(q, mul(dir, -1), style));
	}
	out.push({
		kind: 'text',
		at: add(end, [shoulderSign * style.textGap, -style.textGap]),
		text: a.text,
		anchor: shoulderSign > 0 ? 'start' : 'end',
		baseline: 'auto',
		rotateDeg: 0,
		role: 'note'
	});
	return out;
}

/** A centre mark: a cross, overshooting the marked circle. */
function layoutCentreMark(a, style, tf) {
	const c = tf.toPaper(a.at);
	const arm = (a.half_size ?? 0) * tf.mmPerMeter + style.centreMarkOvershoot;
	return [
		{ kind: 'line', from: [c[0] - arm, c[1]], to: [c[0] + arm, c[1]], role: 'centre' },
		{ kind: 'line', from: [c[0], c[1] - arm], to: [c[0], c[1] + arm], role: 'centre' }
	];
}

/** A centre line: long-dash-dotted, overshooting both ends. */
function layoutCentreLine(a, style, tf) {
	const p0 = tf.toPaper(a.from);
	const p1 = tf.toPaper(a.to);
	const d = norm(sub(p1, p0));
	if (Math.hypot(d[0], d[1]) === 0) return [];
	return [
		{
			kind: 'line',
			from: add(p0, mul(d, -style.centreMarkOvershoot)),
			to: add(p1, mul(d, style.centreMarkOvershoot)),
			role: 'centre'
		}
	];
}

/** A datum: a leader with a filled triangle on the feature, and a boxed letter. */
function layoutDatum(a, style, tf) {
	const p = witnessPoint(a.anchor);
	if (!p) return [];
	const q = tf.toPaper(p);
	const placement = paperPlacement(a.placement, tf);
	const dir =
		Math.hypot(placement[0], placement[1]) > 0
			? norm(placement)
			: /** @type {[number, number]} */ ([0, 1]);
	const end = add(q, mul(dir, style.dimensionOffset));
	const w = style.textHeight * 1.8;
	const h = style.textHeight * 1.8;
	return [
		{ kind: 'line', from: q, to: end, role: 'leader' },
		// The datum triangle: ISO 5459's filled equilateral on the feature.
		...arrowhead(q, mul(dir, -1), { ...style, arrowWidthRatio: 1 }),
		{ kind: 'box', at: [end[0] - w / 2, end[1] - h / 2], width: w, height: h, role: 'datum' },
		{
			kind: 'text',
			at: end,
			text: a.label ?? '',
			anchor: 'middle',
			baseline: 'middle',
			rotateDeg: 0,
			role: 'datum'
		}
	];
}

/**
 * The text primitives for an annotation's value: one line, or two for the
 * stacked forms, plus the basic-dimension box when the format asks (M1).
 *
 * ## Which way "down" is for the second line
 *
 * The second line must read BELOW the first, and the first line's rotation is
 * whatever `readableAngleDeg` chose. Rotating the text's own local down,
 * `[0, 1]`, by that angle gives `[−sin θ, cos θ]`; because the readable angle
 * is always within ±90°, `cos θ ≥ 0`, so that vector always points down the
 * paper. Deriving it from the rotation rather than from the dimension's side
 * is what keeps a dimension above the part and one below it both stacking the
 * same way round.
 *
 * ## And why the block sometimes shifts
 *
 * The anchor a single line uses is one text gap clear of the dimension line.
 * Stacking from there puts the second line ON the dimension line whenever the
 * stack happens to run toward it. So when it does (`down` points back toward
 * the line), the whole block is pushed one line further out — which leaves
 * the line NEAREST the dimension line exactly where a single line would have
 * sat, whichever way round the pair ends up.
 *
 * @param {any} a the annotation, carrying `text`, `textBelow`, `textBoxed`
 * @param {[number, number]} at where a single line would go
 * @param {object} opts
 * @param {[number, number]|null} [opts.away] the paper direction away from the
 *   dimension line; omit for a leadered form, which has nothing to collide with
 * @returns {Primitive[]}
 */
function valueTexts(a, style, at, { anchor, baseline, rotateDeg, away = null, role = 'value' }) {
	const theta = (rotateDeg * Math.PI) / 180;
	const down = /** @type {[number, number]} */ ([-Math.sin(theta), Math.cos(theta)]);
	const lineHeight = style.textHeight * LINE_SPACING;
	const second = a?.textBelow ? String(a.textBelow) : '';

	let first = at;
	if (second && away && dot(down, away) < 0) first = add(at, mul(down, -lineHeight));

	const out = /** @type {Primitive[]} */ ([]);
	// The box goes FIRST so the text is painted over it, not under it.
	if (a?.textBoxed) out.push(basicBox(first, a.text, style, { anchor, baseline, rotateDeg }));
	out.push({
		kind: 'text',
		at: first,
		text: a?.text ?? '',
		anchor,
		baseline,
		rotateDeg,
		role
	});
	if (second) {
		out.push({
			kind: 'text',
			at: add(first, mul(down, lineHeight)),
			text: second,
			anchor,
			baseline,
			rotateDeg,
			role
		});
	}
	return out;
}

/**
 * The rectangle around a BASIC dimension (ISO 129-1 / ASME Y14.5 §2.11: a
 * theoretically exact dimension is enclosed in a box, its variation being
 * controlled by a feature control frame instead).
 *
 * Sized from the style's text height via [`textWidthMm`] and placed to match
 * the text's own anchor and baseline, so the box lands around the glyphs
 * rather than beside them. The baseline cases are approximations of the
 * font's ascent — `middle` is exact by construction, `auto` (alphabetic) puts
 * the glyphs above the anchor, `hanging` below.
 */
function basicBox(at, text, style, { anchor, baseline, rotateDeg }) {
	const pad = style.textGap;
	const width = textWidthMm(text, style.textHeight) + 2 * pad;
	const height = style.textHeight * LINE_SPACING;
	const x = anchor === 'middle' ? at[0] - width / 2 : anchor === 'end' ? at[0] - width + pad : at[0] - pad;
	const y = baseline === 'middle' ? at[1] - height / 2 : baseline === 'hanging' ? at[1] : at[1] - height * 0.8;
	return {
		kind: 'box',
		at: /** @type {[number, number]} */ ([x, y]),
		width,
		height,
		role: 'basic',
		rotateDeg
	};
}

/**
 * A feature control frame (M1, ISO 1101 §6): a leader with an arrowhead on
 * the toleranced feature, and a compartmented rectangle holding the
 * characteristic symbol, the zone and value, and one compartment per datum.
 *
 * The frame is two character heights tall, which is ISO 1101's own
 * proportion, and each compartment is as wide as its text needs plus a gap
 * each side — never narrower than the frame is tall, so the symbol
 * compartment stays square-ish rather than collapsing onto its single glyph.
 *
 * The frame grows in the direction the leader's shoulder runs, so a frame
 * placed to the left of its feature is not drawn back across it.
 */
function layoutFeatureControlFrame(a, style, tf) {
	const p = witnessPoint(a.anchor);
	const cells = Array.isArray(a.cells) ? a.cells.map((c) => String(c ?? '')) : [];
	if (!p || cells.length === 0) return [];
	const q = tf.toPaper(p);
	const placement = paperPlacement(a.placement, tf);
	const dir =
		Math.hypot(placement[0], placement[1]) > 0
			? norm(placement)
			: /** @type {[number, number]} */ ([
					Math.cos((style.leaderAngleDeg * Math.PI) / 180),
					Math.sin((style.leaderAngleDeg * Math.PI) / 180)
				]);
	const knee = add(q, mul(dir, style.dimensionOffset));
	const shoulderSign = dir[0] >= 0 ? 1 : -1;
	const shoulder = add(knee, [shoulderSign * style.leaderShoulder, 0]);

	const out = /** @type {Primitive[]} */ ([
		{ kind: 'line', from: q, to: knee, role: 'leader' },
		{ kind: 'line', from: knee, to: shoulder, role: 'leader' }
	]);
	// A frame leader terminates on the feature the same way a dimension does:
	// an arrow on an edge or a vertex, a dot on a face (ISO 128-22), which the
	// anchor's own shape tells us.
	if (a.anchor?.type === 'Point') {
		out.push({ kind: 'dot', at: q, radius: style.dotRadius, role: 'leader' });
	} else {
		out.push(...arrowhead(q, mul(dir, -1), style));
	}

	const height = style.textHeight * 2;
	const pad = style.textGap;
	const widths = cells.map((c) =>
		Math.max(height * 0.8, textWidthMm(c, style.textHeight) + 2 * pad)
	);
	const total = widths.reduce((s, w) => s + w, 0);
	let x = shoulderSign > 0 ? shoulder[0] : shoulder[0] - total;
	const y = shoulder[1] - height / 2;
	for (let i = 0; i < cells.length; i++) {
		out.push({
			kind: 'box',
			at: /** @type {[number, number]} */ ([x, y]),
			width: widths[i],
			height,
			role: 'frame',
			rotateDeg: 0
		});
		out.push({
			kind: 'text',
			at: /** @type {[number, number]} */ ([x + widths[i] / 2, y + height / 2]),
			text: cells[i],
			anchor: 'middle',
			baseline: 'middle',
			rotateDeg: 0,
			role: 'frame'
		});
		x += widths[i];
	}
	return out;
}

/**
 * A closed filled arrowhead at `tip` pointing along `dir` (so the body of the
 * head is BEHIND the tip), or a 45° architectural tick when the style asks.
 *
 * @param {[number, number]} tip
 * @param {[number, number]} dir unit, paper space
 */
function arrowhead(tip, dir, style) {
	const d = norm(dir);
	if (Math.hypot(d[0], d[1]) === 0) return [];
	if (style.architecturalTicks) {
		// A tick runs at 45° THROUGH the tip, half its length each side.
		const t = /** @type {[number, number]} */ ([
			(d[0] - d[1]) / Math.SQRT2,
			(d[0] + d[1]) / Math.SQRT2
		]);
		const half = style.arrowLength / 2;
		return [
			{ kind: 'line', from: add(tip, mul(t, -half)), to: add(tip, mul(t, half)), role: 'tick' }
		];
	}
	const base = add(tip, mul(d, -style.arrowLength));
	const half = (style.arrowLength * style.arrowWidthRatio) / 2;
	const n = perp(d);
	return [
		{
			kind: 'polygon',
			points: [tip, add(base, mul(n, half)), add(base, mul(n, -half))],
			role: 'arrow'
		}
	];
}

/** A `Placement2` in paper mm (it is authored in view-space meters). */
function paperPlacement(placement, tf) {
	const dx = placement?.dx ?? 0;
	const dy = placement?.dy ?? 0;
	return /** @type {[number, number]} */ ([dx * tf.mmPerMeter, -dy * tf.mmPerMeter]);
}

/**
 * The rotation for text running along `d`, flipped where it would otherwise
 * read upside down — ISO 129-1's rule that a dimension's value is never
 * inverted.
 */
function readableAngleDeg(d) {
	let deg = (Math.atan2(d[1], d[0]) * 180) / Math.PI;
	if (deg > 90) deg -= 180;
	if (deg <= -90) deg += 180;
	return deg;
}

/**
 * How far an ellipse's rim lies from its centre along the paper-space unit
 * direction `dir`, in paper mm.
 *
 * Writing `dir` in the ellipse's own frame as `α û + β ŵ`, a rim point at
 * distance `r` satisfies `(rα/a)² + (rβ/b)² = 1`, so
 * `r = 1 / √((α/a)² + (β/b)²)`. The v-flip into paper space is a reflection,
 * which preserves both radii and perpendicularity, so the axes can be flipped
 * and used directly; `ŵ`'s sign does not matter because only `β²` appears.
 *
 * @param {[number, number]} dir unit, paper space
 */
function ellipseReach(curve, dir, tf) {
	const a = (curve.major_radius ?? 0) * tf.mmPerMeter;
	const b = (curve.minor_radius ?? 0) * tf.mmPerMeter;
	if (!(a > 0) || !(b > 0)) return 0;
	const u = tf.dirToPaper(curve.major_axis ?? [1, 0]);
	if (Math.hypot(u[0], u[1]) === 0) return a;
	const w = perp(u);
	const alpha = dot(dir, u) / a;
	const beta = dot(dir, w) / b;
	const q = Math.hypot(alpha, beta);
	return q > 0 ? 1 / q : 0;
}

/**
 * The four corners of a paper-space `[[x0, y0], [x1, y1]]` box, or none when
 * there is no box. All four are needed, not just the two given: the extreme
 * along an arbitrary direction `n` can be either diagonal.
 *
 * @returns {[number, number][]}
 */
function boxCorners(box) {
	if (!Array.isArray(box) || box.length !== 2) return [];
	const [[x0, y0], [x1, y1]] = box;
	if (![x0, y0, x1, y1].every((v) => Number.isFinite(v))) return [];
	return [
		[x0, y0],
		[x1, y0],
		[x1, y1],
		[x0, y1]
	];
}

/** Intersection of the infinite lines through `(a0, a1)` and `(b0, b1)`. */
function lineIntersection(a0, a1, b0, b1) {
	const r = sub(a1, a0);
	const s = sub(b1, b0);
	const denom = r[0] * s[1] - r[1] * s[0];
	// Parallel (or degenerate): no single intersection.
	if (Math.abs(denom) < 1e-12) return null;
	const qp = sub(b0, a0);
	const t = (qp[0] * s[1] - qp[1] * s[0]) / denom;
	return add(a0, mul(r, t));
}

/**
 * The drawing primitives for one `AnnotationLayout`, in paper mm.
 *
 * `a.text` must already be the formatted value — formatting is `format.js`'s
 * job and happens before layout, so this function never sees a number it
 * could round differently from the one printed. M1 adds three companions on
 * the same terms: `textBelow` (the second line of a stacked form), `textBoxed`
 * (a basic dimension) and `cells` (a feature control frame's compartments).
 *
 * @param {any} a an `AnnotationLayout` with a `text` field added
 * @param {import('./style.js').DrawingStyle} style
 * @param {ReturnType<typeof paperTransform>} tf
 * @param {[number, number]} centrePaper the view's centre, for choosing sides
 * @param {[number, number][]} [boundsPaper] the view's paper-space box,
 *   `[[x0, y0], [x1, y1]]`, which a linear dimension must clear
 * @returns {Primitive[]}
 */
export function layoutAnnotation(a, style, tf, centrePaper, boundsPaper) {
	switch (a?.type) {
		case 'Dimension':
			switch (a.kind?.type) {
				case 'Radius':
					return layoutRadial(a, style, tf, false);
				case 'Diameter':
					return layoutRadial(a, style, tf, true);
				case 'Angle':
					return layoutAngular(a, style, tf);
				case 'Ordinate':
					return layoutOrdinate(a, style, tf);
				default:
					return layoutLinear(a, style, tf, centrePaper, boundsPaper);
			}
		case 'Note':
			return layoutNote(a, style, tf);
		case 'CentreMark':
			return layoutCentreMark(a, style, tf);
		case 'CentreLine':
			return layoutCentreLine(a, style, tf);
		case 'Datum':
			return layoutDatum(a, style, tf);
		case 'FeatureControlFrame':
			return layoutFeatureControlFrame(a, style, tf);
		default:
			// An annotation kind this build does not know. Drawing nothing is
			// the only honest option — a placeholder glyph on a manufacturing
			// drawing is worse than a visible absence.
			return [];
	}
}
