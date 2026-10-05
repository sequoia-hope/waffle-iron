/**
 * Visual view placement on a drawing sheet (`specs/drawings_and_mbd.md` §8,
 * D4e): the two pointer tools that put a view where the cursor is, instead of
 * typing a placement into the panel.
 *
 * Everything here is PURE — paper arithmetic, sector geometry and SVG strings
 * — so the sheet component owns only the pointer events and the store calls.
 * That is the `sheet.js` division (the renderer is a function of the
 * document) and it is also what makes the two tools testable without a
 * browser.
 *
 * ## The two tools
 *
 * - **place-view**: a dialog picks the source, the named view and the scale;
 *   the sheet then shows a dashed ghost box centred on the cursor, snapped to
 *   the sheet grid and to existing views' centres, and a click places the
 *   view there with an explicit `placement_mm`.
 * - **projected-view**: a parent view is clicked, and the cursor's sector
 *   around that view's box picks one of the eight `ProjectedDirection`s — the
 *   four sides and the four isometric corners. The ghost's PLACEMENT and what
 *   the view SHOWS both come from the engine (`probeDrawingViews`), so a
 *   visually placed projected view is byte-identical to a panel-added one and
 *   the first-angle flip is the engine's `projected_frame` rather than a
 *   second copy of the rule here.
 *
 * ## Units
 *
 * Paper millimetres, `+x` right and `+y` UP from the sheet's bottom-left
 * corner — `DrawingView.placement_mm`'s own convention. The flip to SVG's
 * y-down happens in one expression per function that emits markup, as it does
 * in `sheet.js`.
 */

/**
 * The ISO 5455 scale series, as `{ ratio, scale }` with `scale` the paper
 * length per model length that `DrawingView.scale` holds.
 *
 * The standard's series, and every entry is on it: a drawing at a scale off
 * the series is one a reader cannot check against a scale rule, which is why
 * each enlargement and reduction is 1, 2 or 5 times a power of ten.
 *
 * It stops at 1:1000 (D4e review, which found the list described as the
 * standard's whole one). ISO 5455:1979 continues 1:2000, 1:5000, 1:10000 —
 * reductions for civil work, where a 1:5000 site plan is a normal drawing and
 * a 1:5000 machine part is not a drawing at all. They are reachable through
 * the free field, which exists anyway because a detail at 3:1 is a thing
 * people draw and refusing it would push them back to the numeric panel the
 * tool is meant to replace.
 */
export const ISO_5455_SCALES = [
	{ ratio: '50:1', scale: 50 },
	{ ratio: '20:1', scale: 20 },
	{ ratio: '10:1', scale: 10 },
	{ ratio: '5:1', scale: 5 },
	{ ratio: '2:1', scale: 2 },
	{ ratio: '1:1', scale: 1 },
	{ ratio: '1:2', scale: 0.5 },
	{ ratio: '1:5', scale: 0.2 },
	{ ratio: '1:10', scale: 0.1 },
	{ ratio: '1:20', scale: 0.05 },
	{ ratio: '1:50', scale: 0.02 },
	{ ratio: '1:100', scale: 0.01 },
	{ ratio: '1:200', scale: 0.005 },
	{ ratio: '1:500', scale: 0.002 },
	{ ratio: '1:1000', scale: 0.001 }
];

/**
 * The placement grid, in paper millimetres: 5 mm, the grid a drafting sheet
 * is laid out on (ISO 5457's frame divisions are 10 mm, and half of one is
 * the finest step that still reads as deliberate).
 *
 * A grid in PAPER units rather than model units, for the reason D4d's pick
 * radius is a paper distance: a 1:10 view and a 2:1 view must snap alike, and
 * a model-space grid would put the two views' alignment on different pitches.
 */
export const SHEET_GRID_MM = 5;

/**
 * How near a cursor has to be to an existing view's centre line, in paper
 * millimetres, before the ghost snaps onto it.
 *
 * Larger than the grid step, deliberately: alignment with an existing view is
 * what a drafter actually wants from the snap (a projection group is read by
 * its rows and columns), so it has to win over the grid when both are in
 * reach.
 */
export const ALIGN_SNAP_MM = 8;

/**
 * `ProjectedDirection`'s eight tags, in `ProjectedDirection::ALL`'s order,
 * with the label the engine gives each (`ProjectedDirection::label`).
 *
 * The labels are a MIRROR of the Rust table, pinned by
 * `crates/feature-engine/tests/js_projected_direction_mirror.rs` — the
 * `js_format_mirror` arrangement, for the same reason: two copies of a label
 * the user reads will drift, and the test is cheaper than the drift. A ghost
 * whose label comes back from a probe uses the ENGINE's string; this table is
 * for the hover, which must label a sector before any query answers.
 */
export const PROJECTED_DIRECTIONS = [
	{ tag: 'Left', label: 'Left', step: [-1, 0] },
	{ tag: 'Right', label: 'Right', step: [1, 0] },
	{ tag: 'Up', label: 'Up', step: [0, 1] },
	{ tag: 'Down', label: 'Down', step: [0, -1] },
	{ tag: 'UpLeft', label: 'Iso (up-left)', step: [-1, 1] },
	{ tag: 'UpRight', label: 'Iso (up-right)', step: [1, 1] },
	{ tag: 'DownLeft', label: 'Iso (down-left)', step: [-1, -1] },
	{ tag: 'DownRight', label: 'Iso (down-right)', step: [1, -1] }
];

/**
 * The label for one `ProjectedDirection` tag, or the tag itself for one this
 * build does not know (a document written by a newer build).
 * @param {string} tag
 * @returns {string}
 */
export function directionLabel(tag) {
	return PROJECTED_DIRECTIONS.find((d) => d.tag === tag)?.label ?? String(tag ?? '?');
}

/**
 * The two tools' descriptors, for the drawing toolbar (D4d builds the toolbar
 * from an array of exactly this shape; wiring these in is one import).
 *
 * `mode` is the sheet pointer mode the tool enters, which is what
 * `sheetMode` dispatches on.
 */
export const VIEW_PLACEMENT_TOOLS = [
	{
		id: 'place-view',
		mode: 'place-view',
		label: '+ view…',
		title: 'Place a view of a part on the sheet: pick the source and scale, then click where it goes',
		testid: 'dwg-tool-place-view'
	},
	{
		id: 'project-view',
		mode: 'project-view',
		label: 'Project from…',
		title: 'Project a view from one already on the sheet: click the parent, then hover a side or a corner',
		testid: 'dwg-tool-project-view'
	}
];

/**
 * An orthonormal paper basis from a view frame's `dir` and `up`, by the SAME
 * rule `waffle_types::kernel::projection::ViewFrame::basis` uses: `w` is the
 * unit line of sight, `v` is `up` Gram-Schmidted against it, and `u = w × v`
 * so that `(u, v, −w)` is right-handed.
 *
 * Re-derived here rather than sent by the engine because it is three lines of
 * linear algebra over two vectors the probe already answers with, and because
 * a `u`/`v` pair on the wire would be a second representation of the frame
 * that could disagree with the one the view is actually projected with. If
 * the two vectors are parallel (no basis), `null` — the caller then has no
 * ghost to draw, which is the honest outcome.
 *
 * @param {[number, number, number]} dir
 * @param {[number, number, number]} up
 * @returns {{ u: number[], v: number[], w: number[] } | null}
 */
export function paperBasis(dir, up) {
	const unit = (a) => {
		const n = Math.hypot(a[0], a[1], a[2]);
		return n > 0 && Number.isFinite(n) ? [a[0] / n, a[1] / n, a[2] / n] : null;
	};
	const w = unit(Array.isArray(dir) ? dir.map(Number) : []);
	if (!w) return null;
	const u0 = Array.isArray(up) ? up.map(Number) : [];
	if (u0.length !== 3 || !u0.every(Number.isFinite)) return null;
	const t = u0[0] * w[0] + u0[1] * w[1] + u0[2] * w[2];
	const v = unit([u0[0] - t * w[0], u0[1] - t * w[1], u0[2] - t * w[2]]);
	if (!v) return null;
	const u = [w[1] * v[2] - w[2] * v[1], w[2] * v[0] - w[0] * v[2], w[0] * v[1] - w[1] * v[0]];
	return { u, v, w };
}

/**
 * The ghost box's size in paper millimetres: the source's world AABB
 * projected through the view's frame, times the scale.
 *
 * ## Why this is an UPPER bound, and why that is the safe direction
 *
 * The solid lies inside its AABB, and an orthographic projection is linear,
 * so the projection of the solid lies inside the projection of the AABB —
 * whose 2D extent is the extent of its eight projected corners (the image of
 * a box under a linear map is the convex hull of the images of its corners).
 * So this box CONTAINS the drawn curves, and a view placed by it cannot
 * overlap a neighbour the ghost said it would clear.
 *
 * **The slack is real, and on a curved body it is large** (D4e review,
 * measured in `wasm-bridge/tests/tool_drawing.rs::the_probes_bounds_of_a_
 * curved_body_are_an_analytic_upper_bound_and_a_loose_one`). A cylinder seen
 * down its axis draws a circle inside a square ghost, which is the cheap
 * case; seen from the side its ghost is `2r + h` tall where the part is `h`
 * tall, because the kernel's conservative box grows a circular edge by its
 * radius in ALL THREE axes. For a radius-12, height-6 mm cylinder that is a
 * ghost five times too tall. It errs in the safe direction for layout and it
 * is not corrected here — a consumer cannot un-widen a box it is handed, and
 * the fix is a tighter analytic box in the kernel.
 *
 * The one caveat in the OTHER direction is the AABB's provenance. The engine
 * answers with the kernel's `solid_aabb`, which is conservative for an
 * analytic solid — but a body it declines on (one carrying a surface-pair or
 * hyperbola edge) falls back to the render tessellation's bounds, and a
 * tessellation is INSCRIBED, so that fallback is short of the true extent by
 * the chord deficit (≈ the sagitta of one facet). The ghost is then under by
 * that much, which is why D4e's oracle compares the ghost with the drawn
 * extent "to within the mesh-vs-analytic deficit" rather than asserting
 * containment.
 *
 * @param {object} input
 * @param {[[number,number,number],[number,number,number]] | null | undefined} input.boundsM
 *   the source's world AABB `[min, max]` in METERS
 * @param {[number, number, number]} input.dir the view frame's line of sight
 * @param {[number, number, number]} input.up the view frame's paper up
 * @param {number} [input.scale] paper length per model length (default 1)
 * @returns {[number, number] | null} `[width, height]` in paper mm
 */
export function ghostExtentMm({ boundsM, dir, up, scale = 1 }) {
	const basis = paperBasis(dir, up);
	const s = Number(scale);
	if (!basis || !Number.isFinite(s) || s <= 0) return null;
	const lo = boundsM?.[0];
	const hi = boundsM?.[1];
	if (!Array.isArray(lo) || !Array.isArray(hi)) return null;
	if (![...lo, ...hi].every((x) => Number.isFinite(Number(x)))) return null;
	let minU = Infinity;
	let maxU = -Infinity;
	let minV = Infinity;
	let maxV = -Infinity;
	for (let i = 0; i < 8; i += 1) {
		const p = [
			Number(i & 1 ? hi[0] : lo[0]),
			Number(i & 2 ? hi[1] : lo[1]),
			Number(i & 4 ? hi[2] : lo[2])
		];
		const u = p[0] * basis.u[0] + p[1] * basis.u[1] + p[2] * basis.u[2];
		const v = p[0] * basis.v[0] + p[1] * basis.v[1] + p[2] * basis.v[2];
		minU = Math.min(minU, u);
		maxU = Math.max(maxU, u);
		minV = Math.min(minV, v);
		maxV = Math.max(maxV, v);
	}
	// Meters → millimetres, then the drawing scale.
	return [(maxU - minU) * 1000 * s, (maxV - minV) * 1000 * s];
}

/**
 * A view's drawn extent in paper millimetres, from its cached layout — the
 * EXACT box, for a view that has been projected.
 *
 * `cache.bbox` is `[[minU, minV], [maxU, maxV]]` in view-plane meters, so the
 * same two factors apply as in `ghostExtentMm`. `null` for a view the engine
 * could not rebuild, which is a view with no box to hit-test against.
 *
 * @param {any} view a `DrawingView` as the store carries it
 * @returns {[number, number] | null}
 */
export function viewExtentMm(view) {
	const box = view?.cache?.bbox;
	const s = Number(view?.scale ?? 1);
	if (!Array.isArray(box) || box.length !== 2 || !Number.isFinite(s) || s <= 0) return null;
	const [min, max] = box;
	if (!Array.isArray(min) || !Array.isArray(max)) return null;
	const w = (Number(max[0]) - Number(min[0])) * 1000 * s;
	const h = (Number(max[1]) - Number(min[1])) * 1000 * s;
	return [w, h].every(Number.isFinite) ? [w, h] : null;
}

/**
 * Snap a placement to the sheet grid and to existing views' centre lines.
 *
 * Alignment wins over the grid, per axis and independently: a ghost one
 * cursor-width from a view's column snaps into the column but keeps the row
 * the cursor asked for. Done per axis rather than per point because that is
 * what a projection group needs — a view placed to the right of another
 * shares its row and nothing else.
 *
 * @param {object} input
 * @param {[number, number]} input.atMm the cursor, in paper mm
 * @param {any[]} [input.views] the sheet's views (their `placement_mm` are the
 *   centre lines)
 * @param {string | null} [input.exceptId] a view to ignore (the one being moved)
 * @param {number} [input.gridMm]
 * @param {number} [input.alignMm]
 * @returns {{ mm: [number, number], alignedTo: [string | null, string | null] }}
 */
export function snapPlacementMm({
	atMm,
	views = [],
	exceptId = null,
	gridMm = SHEET_GRID_MM,
	alignMm = ALIGN_SNAP_MM
}) {
	const out = /** @type {[number, number]} */ ([Number(atMm?.[0]) || 0, Number(atMm?.[1]) || 0]);
	const alignedTo = /** @type {[string | null, string | null]} */ ([null, null]);
	for (const axis of [0, 1]) {
		let best = null;
		for (const view of views) {
			if (!view || view.id === exceptId) continue;
			const c = Number(view.placement_mm?.[axis]);
			if (!Number.isFinite(c)) continue;
			const d = Math.abs(c - out[axis]);
			if (d <= alignMm && (best === null || d < best.d)) best = { d, c, id: view.id };
		}
		if (best) {
			out[axis] = best.c;
			alignedTo[axis] = best.id ?? null;
			continue;
		}
		if (Number.isFinite(gridMm) && gridMm > 0) out[axis] = Math.round(out[axis] / gridMm) * gridMm;
	}
	return { mm: out, alignedTo };
}

/**
 * Which of the eight sectors around a view's box the cursor is in, as a
 * `ProjectedDirection` tag — or `null` for a cursor INSIDE the box.
 *
 * The box's own edge lines extended to infinity cut the plane into nine
 * regions: the box, four side sectors and four corner sectors. That is the
 * division a drafter already has in mind (a view "to the right of" another is
 * the one beyond its right edge, at its own height), and it needs no angles
 * and no tuning — a cursor is in the corner sector exactly when it is clear
 * of both edges, which is also exactly when an isometric placed there would
 * not collide with the side views.
 *
 * @param {object} input
 * @param {[number, number]} input.atMm the cursor, in paper mm
 * @param {[number, number]} input.centreMm the view's placement
 * @param {[number, number]} input.extentMm the view's drawn extent
 * @returns {string | null}
 */
export function sectorAtMm({ atMm, centreMm, extentMm }) {
	const dx = Number(atMm?.[0]) - Number(centreMm?.[0]);
	const dy = Number(atMm?.[1]) - Number(centreMm?.[1]);
	const hw = Math.abs(Number(extentMm?.[0] ?? 0)) / 2;
	const hh = Math.abs(Number(extentMm?.[1] ?? 0)) / 2;
	if (![dx, dy, hw, hh].every(Number.isFinite)) return null;
	const sx = dx > hw ? 1 : dx < -hw ? -1 : 0;
	const sy = dy > hh ? 1 : dy < -hh ? -1 : 0;
	if (sx === 0 && sy === 0) return null;
	return PROJECTED_DIRECTIONS.find((d) => d.step[0] === sx && d.step[1] === sy)?.tag ?? null;
}

/**
 * The paper point under a pointer event, in sheet millimetres measured from
 * the BOTTOM-left corner.
 *
 * The sheet's `<svg>` carries its size in `mm` and a `viewBox` of the paper
 * extent, and the CSS scales it to fit; so the rendered rect and the viewBox
 * are the same aspect and one ratio converts both axes. Read off the rect
 * rather than through `getScreenCTM` because the markup is injected as a
 * string and the element is the only handle the component has.
 *
 * @param {DOMRect} rect the sheet `<svg>`'s bounding rect
 * @param {{ clientX: number, clientY: number }} event
 * @param {[number, number]} sheetMm the paper `[width, height]`
 * @returns {[number, number] | null}
 */
export function paperPointMm(rect, event, sheetMm) {
	if (!rect || !(rect.width > 0) || !(rect.height > 0)) return null;
	const [wMm, hMm] = [Number(sheetMm?.[0]), Number(sheetMm?.[1])];
	if (!(wMm > 0) || !(hMm > 0)) return null;
	const x = ((Number(event?.clientX) - rect.left) / rect.width) * wMm;
	// SVG measures y down from the top; a placement is measured up from the
	// bottom (`sheet.js`'s one flip, read the other way).
	const y = hMm - ((Number(event?.clientY) - rect.top) / rect.height) * hMm;
	return [x, y].every(Number.isFinite) ? [x, y] : null;
}

/** XML-escape, for the ghost's label. Mirrors `svg.js`'s `esc`. */
function esc(s) {
	return String(s ?? '')
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;');
}

/** A number as SVG markup: finite, trimmed. Mirrors `svg.js`'s `n`. */
function num(x) {
	const v = Number(x);
	return Number.isFinite(v) ? String(Math.round(v * 1000) / 1000) : '0';
}

/**
 * The ghost as SVG markup, in the SHEET's own user units (paper mm, y down),
 * ready to be spliced in before the sheet's closing tag.
 *
 * A dashed rectangle plus a label above it, and a centre cross so a user can
 * see what the snap did — the rectangle alone reads as a region rather than
 * as a placement. Returns `''` when there is nothing to draw, so a caller can
 * concatenate unconditionally.
 *
 * @param {object} input
 * @param {[number, number]} input.centreMm where the view would sit
 * @param {[number, number] | null} input.extentMm its drawn extent
 * @param {[number, number]} input.sheetMm the paper `[width, height]`
 * @param {string} [input.label]
 * @param {boolean} [input.aligned] whether the placement snapped onto a view
 * @returns {string}
 */
export function ghostSvg({ centreMm, extentMm, sheetMm, label = '', aligned = false }) {
	const [cx, cy] = [Number(centreMm?.[0]), Number(centreMm?.[1])];
	const hMm = Number(sheetMm?.[1]);
	if (![cx, cy, hMm].every(Number.isFinite)) return '';
	// A view whose extent is unknown still gets a mark: the placement is the
	// decided thing, and a ghost that vanishes would read as "cannot place
	// here" when the truth is "cannot size the box yet".
	const w = Math.abs(Number(extentMm?.[0] ?? 0));
	const h = Math.abs(Number(extentMm?.[1] ?? 0));
	const sized = w > 0 && h > 0 && Number.isFinite(w) && Number.isFinite(h);
	const x = cx - w / 2;
	// The flip: a centre measured up from the bottom becomes a top edge
	// measured down from the top.
	const y = hMm - cy - h / 2;
	const colour = aligned ? '#2e7d32' : '#1565c0';
	const parts = [];
	if (sized) {
		parts.push(
			`<rect class="wi-ghost-box" x="${num(x)}" y="${num(y)}" width="${num(w)}" height="${num(h)}" ` +
				`fill="none" stroke="${colour}" stroke-width="0.4" stroke-dasharray="3 2" />`
		);
	}
	const cross = 3;
	parts.push(
		`<line class="wi-ghost-cross" x1="${num(cx - cross)}" y1="${num(hMm - cy)}" ` +
			`x2="${num(cx + cross)}" y2="${num(hMm - cy)}" stroke="${colour}" stroke-width="0.3" />`,
		`<line class="wi-ghost-cross" x1="${num(cx)}" y1="${num(hMm - cy - cross)}" ` +
			`x2="${num(cx)}" y2="${num(hMm - cy + cross)}" stroke="${colour}" stroke-width="0.3" />`
	);
	if (label) {
		parts.push(
			`<text class="wi-ghost-label" x="${num(cx)}" y="${num(y - 1.5)}" font-size="3.5" ` +
				`font-family="Helvetica, Arial, sans-serif" fill="${colour}" text-anchor="middle">` +
				`${esc(label)}</text>`
		);
	}
	return (
		`<g class="wi-ghost" data-testid="dwg-ghost" data-label="${esc(label)}" ` +
		`data-centre-mm="${num(cx)},${num(cy)}" ` +
		`data-extent-mm="${sized ? `${num(w)},${num(h)}` : ''}" ` +
		`data-aligned="${aligned ? 'yes' : 'no'}" pointer-events="none">${parts.join('')}</g>`
	);
}

/**
 * Splice ghost markup into a rendered sheet, before its closing `</svg>`.
 *
 * A string splice rather than a second overlaid `<svg>`: the sheet's element
 * is sized by CSS (`max-width: 100%`), so an overlay would have to re-measure
 * it every frame to line up, and a one-pixel disagreement in a tool that
 * SHOWS you where something will land is the whole of the tool's value. In
 * the sheet's own user units the ghost cannot be out by anything.
 *
 * @param {string} svg the sheet, as `renderSheetSvg` returned it
 * @param {string} ghost
 * @returns {string}
 */
export function withGhost(svg, ghost) {
	const text = String(svg ?? '');
	if (!ghost) return text;
	const at = text.lastIndexOf('</svg>');
	return at < 0 ? text : text.slice(0, at) + ghost + text.slice(at);
}
