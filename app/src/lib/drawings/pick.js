/**
 * Picking on the sheet (`specs/drawings_and_mbd.md` §8, D4d): which anchor a
 * click would bind to, and which existing annotation it would select.
 *
 * ## Paper space, and nothing else
 *
 * Every distance in this module is a PAPER MILLIMETRE. That is the whole
 * point of the increment's pick rule: a view drawn at 1:10 and a detail at
 * 2:1 must pick alike, and a model distance would make the 2:1 view twenty
 * times fussier than the 1:10 one about the same gesture. The caller converts
 * once, at the DOM boundary (`DrawingSheet.svelte` inverts the nested view
 * `<svg>`'s own `getScreenCTM()`, whose user units ARE paper mm by
 * construction — `renderViewSvg` writes a `viewBox` the same size as its
 * `width` in mm), and everything here works in the converted units.
 *
 * A consequence worth stating because it is the oracle: the pick radius is
 * constant in paper mm, so it is constant on the PRINTED sheet and varies in
 * screen pixels with the zoom. That is the behaviour a drafter expects and
 * the one `drawing-pick-radius.spec.js` measures.
 *
 * ## Pure
 *
 * No DOM, no store, no `Date`, no iteration over unordered keys. Same anchors
 * and same point in, same pick out — which is what lets the refusals be
 * tested without a browser and asserted in one.
 */
import { annotationHandles, distanceToSegment, paperTransform } from './layout.js';
import { annotationText } from './svg.js';
import { drawingStyle } from './style.js';

/**
 * The pick radius, in PAPER millimetres.
 *
 * 2 mm is about a dimension line's own extension gap and a little under a
 * text height, so an anchor is reachable without being so greedy that two
 * edges of a small feature are both always in range (which would make the tie
 * refusal the normal outcome rather than the exceptional one).
 */
export const PICK_RADIUS_MM = 2;

/**
 * Two candidates whose distances differ by less than this are a TIE, in paper
 * mm.
 *
 * It exists because "the nearest wins" is not a decision when the nearest is
 * nearer by a rounding error: two coincident projected edges (a plate seen
 * edge-on puts a wall's witness point exactly on another's) would otherwise
 * be resolved by whichever the engine happened to list first, and binding a
 * dimension to the wrong edge is the silent wrong this whole spec exists to
 * prevent. 0.15 mm is well under the smallest gap a drafter can aim at and
 * well over any arithmetic noise in a projection.
 */
export const PICK_TIE_MM = 0.15;

/**
 * The view-space → paper-space map for one drawing view, as its own nested
 * `<svg>` uses it.
 *
 * The origin is the view's bbox top-left (`[minU, maxV]`), which is exactly
 * what `renderViewSvg` passes `paperTransform`: paper `(0, 0)` is that
 * corner, and the SVG's `viewBox` then starts at `-pad`. A view with no
 * bbox (one that drew nothing) still gets a transform centred on the origin,
 * so an annotation-only view is not a special case for the caller.
 *
 * @param {any} view a `DrawingView` with its `cache`
 * @returns {ReturnType<typeof paperTransform>}
 */
export function viewPaperTransform(view) {
	const bbox = view?.cache?.bbox ?? null;
	const origin =
		Array.isArray(bbox) && bbox.length === 2
			? /** @type {[number, number]} */ ([Number(bbox[0][0]), Number(bbox[1][1])])
			: /** @type {[number, number]} */ ([0, 0]);
	return paperTransform({ scale: Number(view?.scale) || 1, origin });
}

/**
 * The view's own paper-space box and centre, as `layoutAnnotation` derives
 * them — needed to place a dimension and to find where an existing one was
 * drawn.
 *
 * @param {any} view
 * @param {ReturnType<typeof paperTransform>} tf
 * @returns {{ centre: [number, number], bounds: [number, number][] | undefined }}
 */
export function viewPaperBox(view, tf) {
	const bbox = view?.cache?.bbox ?? null;
	if (!Array.isArray(bbox) || bbox.length !== 2) {
		return { centre: [0, 0], bounds: undefined };
	}
	const [[minU, minV], [maxU, maxV]] = bbox;
	const w = (Number(maxU) - Number(minU)) * tf.mmPerMeter;
	const h = (Number(maxV) - Number(minV)) * tf.mmPerMeter;
	return {
		centre: [w / 2, h / 2],
		bounds: [
			[0, 0],
			[w, h]
		]
	};
}

/**
 * The anchors of one view that can be PICKED, each with its paper-mm point.
 *
 * An anchor with no `at` is left out, and that is the engine's own refusal
 * carried forward rather than a gap here: a vertex anchor's position is
 * resolved per annotation (`ViewAnchor.at` is deliberately `None` for one),
 * and a sampled polyline has no canonical witness point at all — its midpoint
 * moves with the chord tolerance that sampled it, so a dimension anchored on
 * one would read a different number at a different render density. Neither is
 * offered to a click rather than being offered and then refused.
 *
 * @param {any[]} anchors the view's `ViewAnchor` list
 * @param {ReturnType<typeof paperTransform>} tf
 * @returns {{ anchor: any, paper: [number, number] }[]}
 */
export function pickableAnchors(anchors, tf) {
	const out = [];
	for (const anchor of anchors ?? []) {
		const at = anchor?.at;
		if (!Array.isArray(at) || at.length !== 2 || !at.every(Number.isFinite)) continue;
		const paper = tf.toPaper([Number(at[0]), Number(at[1])]);
		if (!paper.every(Number.isFinite)) continue;
		out.push({ anchor, paper });
	}
	return out;
}

/**
 * What an anchor IS, in the words the hover hint uses.
 *
 * The `shape` (what the projection made of the entity) leads, because that is
 * what decides whether a tool can use it; the model `kind` disambiguates the
 * two that look alike on paper (a face's silhouette and an edge both draw a
 * curve).
 *
 * @param {any} anchor a `ViewAnchor`
 * @returns {string}
 */
export function anchorKindLabel(anchor) {
	const shape = anchor?.shape?.type ?? '?';
	const kind = anchor?.kind?.type ?? '?';
	switch (shape) {
		case 'Line':
			return kind === 'Face' ? 'silhouette' : 'edge';
		case 'Circle':
			return 'circle';
		case 'Ellipse':
			return 'circle (oblique)';
		case 'Point':
			return kind === 'Vertex' ? 'vertex' : 'edge seen end-on';
		case 'Polyline':
			return 'sampled curve';
		default:
			return shape.toLowerCase();
	}
}

/**
 * Pick the nearest anchor to `paper` within `radiusMm`.
 *
 * Three outcomes, and the third is the one that matters:
 *
 * - `{ anchor, distance }` — one nearest candidate.
 * - `null` — nothing in range.
 * - `{ tie: [...] }` — two or more candidates equally near (within
 *   [`PICK_TIE_MM`]). REFUSED rather than resolved: the drawing cannot say
 *   which the drafter meant, and a dimension bound to the wrong edge prints a
 *   plausible number for the wrong feature. The caller shows the candidates
 *   as a hint so the gesture can be repeated somewhere less ambiguous.
 *
 * @param {{ anchor: any, paper: [number, number] }[]} candidates
 * @param {[number, number]} paper
 * @param {{ radiusMm?: number, tieMm?: number }} [opts]
 * @returns {{ anchor: any, distance: number } | { tie: any[] } | null}
 */
export function pickAnchor(candidates, paper, opts) {
	const radius = opts?.radiusMm ?? PICK_RADIUS_MM;
	const tie = opts?.tieMm ?? PICK_TIE_MM;
	const inRange = [];
	for (const c of candidates ?? []) {
		const distance = Math.hypot(c.paper[0] - paper[0], c.paper[1] - paper[1]);
		if (distance <= radius) inRange.push({ anchor: c.anchor, distance });
	}
	if (inRange.length === 0) return null;
	inRange.sort((a, b) => a.distance - b.distance);
	const best = inRange[0];
	const tied = inRange.filter((c) => c.distance - best.distance <= tie);
	if (tied.length > 1) return { tie: tied.map((c) => c.anchor) };
	return best;
}

/**
 * Can `tool` bind to this anchor? `null` when it can, else the reason.
 *
 * The check is here, at the pick, rather than left to the engine's own
 * measurement refusal, because the hover hint has to say WHY before the click
 * — "a radius needs a circular rim" read under the cursor is a usable
 * instruction, where the same sentence as an error toast after the annotation
 * was authored and rolled back is a report of something that already went
 * wrong.
 *
 * @param {string} tool a `sheetMode` dimension tool id
 * @param {any} anchor
 * @returns {string | null}
 */
export function anchorRefusal(tool, anchor) {
	const shape = anchor?.shape?.type ?? '?';
	switch (tool) {
		case 'dimension-radius':
		case 'dimension-diameter':
			return shape === 'Circle' || shape === 'Ellipse'
				? null
				: 'only a circular rim has a radius — pick a circle or a hole';
		case 'dimension-angle':
			return shape === 'Line' ? null : 'an angle is measured between two straight edges';
		default:
			return shape === 'Polyline'
				? 'a sampled curve has no witness point to measure from'
				: null;
	}
}

/**
 * The existing annotation nearest `paper`, within `radiusMm`, or `null`.
 *
 * `handles` comes from [`annotationGrabHandles`], which runs the real layout:
 * what a click selects is what the sheet drew.
 *
 * @param {{ index: number, points: [number, number][],
 *           segments: [[number, number], [number, number]][] }[]} handles
 * @param {[number, number]} paper
 * @param {number} [radiusMm]
 * @returns {{ index: number, distance: number } | null}
 */
export function pickAnnotation(handles, paper, radiusMm = PICK_RADIUS_MM) {
	let best = null;
	for (const h of handles ?? []) {
		let d = Infinity;
		for (const p of h.points) d = Math.min(d, Math.hypot(p[0] - paper[0], p[1] - paper[1]));
		for (const [a, b] of h.segments) d = Math.min(d, distanceToSegment(paper, a, b));
		if (d <= radiusMm && (best === null || d < best.distance)) {
			best = { index: h.index, distance: d };
		}
	}
	return best;
}

/**
 * Where each of a view's drawn annotations can be grabbed, in paper mm.
 *
 * The `text` each annotation shows is formatted first, with the same
 * `annotationText` the renderer uses: a dimension's primitives include its
 * text primitive, and a layout run without the text would place it at a
 * different width. (It does not affect the point the text is anchored AT, but
 * running the layout a second way is exactly the drift this module refuses.)
 *
 * @param {any} view a `DrawingView` with its `cache`
 * @param {{ unit?: string, documentPrecision?: number,
 *           style?: Partial<import('./style.js').DrawingStyle> }} [opts]
 * @returns {{ index: number, points: [number, number][],
 *             segments: [[number, number], [number, number]][] }[]}
 */
export function annotationGrabHandles(view, opts) {
	const style = drawingStyle(opts?.style);
	const tf = viewPaperTransform(view);
	const { centre, bounds } = viewPaperBox(view, tf);
	const unit = opts?.unit ?? 'mm';
	const documentPrecision = opts?.documentPrecision ?? 2;
	const out = [];
	(view?.cache?.annotations ?? []).forEach((a, index) => {
		const text = annotationText(a, { unit, documentPrecision });
		const { points, segments } = annotationHandles({ ...a, text }, style, tf, centre, bounds);
		if (points.length === 0 && segments.length === 0) return;
		out.push({ index, points, segments });
	});
	return out;
}

/**
 * How many anchors `tool` takes, and whether it ends with a placement click.
 *
 * One table, read by the toolbar (for its hint), by the pointer dispatch (to
 * know when a pick flow is complete) and by the spec. A tool whose arity is
 * stated in two places is a tool that eventually disagrees with itself about
 * when it is finished.
 *
 * `Angle` takes NO placement click: `layoutAngular` swings its arc from the
 * two edges' own intersection and reads no placement, so a third click would
 * store a number nothing draws (see `placementForPoint`).
 *
 * @type {Record<string, { anchors: number, placement: boolean, optional?: boolean }>}
 */
export const TOOL_FLOW = {
	'dimension-distance': { anchors: 2, placement: true },
	'dimension-hdistance': { anchors: 2, placement: true },
	'dimension-vdistance': { anchors: 2, placement: true },
	'dimension-pointline': { anchors: 2, placement: true },
	'dimension-angle': { anchors: 2, placement: false },
	'dimension-radius': { anchors: 1, placement: true },
	'dimension-diameter': { anchors: 1, placement: true },
	note: { anchors: 1, placement: true, optional: true },
	datum: { anchors: 1, placement: true }
};

/**
 * The annotation spec `addDrawingAnnotation` takes for `tool`, given its
 * picked anchors.
 *
 * There is no `value` and there never can be: the engine measures the
 * annotation from the model on every rebuild and refuses a literal
 * (`check_measured`). This function's whole job is to name the entities and
 * the kind.
 *
 * @param {string} tool
 * @param {any[]} anchors the picked `ViewAnchor`s, in pick order
 * @param {{ text?: string, label?: string, placement?: [number, number] }} [extra]
 * @returns {Record<string, any> | null}
 */
export function annotationSpecFor(tool, anchors, extra) {
	const picks = (anchors ?? []).map((a) => ({ pid: a.pid, kind: a.kind?.type ?? 'Edge' }));
	const placement = extra?.placement ?? null;
	switch (tool) {
		case 'dimension-distance':
		case 'dimension-hdistance':
		case 'dimension-vdistance':
		case 'dimension-pointline':
		case 'dimension-angle':
		case 'dimension-radius':
		case 'dimension-diameter': {
			const kind = DIMENSION_KIND[tool];
			if (!kind) return null;
			return { annotation: 'Dimension', kind, anchors: picks, placement };
		}
		case 'note':
			return {
				annotation: 'Note',
				anchors: picks,
				text: extra?.text ?? '',
				placement
			};
		case 'datum':
			return {
				annotation: 'Datum',
				anchors: picks,
				label: extra?.label ?? 'A',
				placement
			};
		default:
			return null;
	}
}

/**
 * Tool id → the `DimensionKind` tag the engine takes.
 *
 * Every value here is in `wasm_bridge::tools::drawing::DIMENSION_TAGS`, which
 * is the authorable set. `Ordinate` is NOT, and so has no tool: it reads one
 * raw view-plane coordinate measured from the view FRAME's origin, so its
 * printed value cannot be read off the sheet and moves when the part moves in
 * space (§7's open item). It joins when that closes.
 */
export const DIMENSION_KIND = {
	'dimension-distance': 'Distance',
	'dimension-hdistance': 'HDistance',
	'dimension-vdistance': 'VDistance',
	'dimension-pointline': 'PointLineDistance',
	'dimension-angle': 'Angle',
	'dimension-radius': 'Radius',
	'dimension-diameter': 'Diameter'
};

/**
 * The paper-mm point an annotation's placement should be measured to, given
 * where the user clicked and which annotation is being placed — i.e. the
 * `target` argument of `placementForPoint`.
 *
 * A thin alias today (the click IS the target), kept as its own name because
 * a snap (to the sheet grid, or to another dimension's line — the usual next
 * refinement) belongs here and nowhere else.
 *
 * @param {[number, number]} paper
 * @returns {[number, number]}
 */
export function placementTarget(paper) {
	return [paper[0], paper[1]];
}
