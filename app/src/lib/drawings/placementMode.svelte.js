/**
 * The sheet's pointer MODES for D4e's two placement tools
 * (`specs/drawings_and_mbd.md` §8): the state a hover and a click run through,
 * beside the pure paper arithmetic in `viewPlacement.js`.
 *
 * Why a module and not component state: the tool is STARTED somewhere else
 * (the panel's two buttons today, D4d's drawing toolbar after that) and runs
 * on the SHEET, and a mode that lived in either component would have to be
 * threaded through the page to reach the other. Everything the sheet needs is
 * `placementGhost()` plus the three handlers, so wiring this into the toolbar
 * is one import either way.
 *
 * The engine answers two questions this module never decides for itself:
 * where a projected view goes (`placement_mm`) and what it shows (`shows`).
 * Both come back from `probeDrawingView`, which runs the same
 * `default_placement` an add does — so a view placed by pointing is the view
 * the panel would have added, and the first-angle flip is the engine's table.
 */
import {
	addDrawingView,
	getDocumentRevision,
	getDocumentTabs,
	getDrawingSheet,
	probeDrawingView
} from '$lib/engine/store.svelte.js';
import { sheetExtentMm } from './sheet.js';
import {
	directionLabel,
	ghostSvg,
	sectorAtMm,
	snapPlacementMm,
	ghostExtentMm,
	viewExtentMm,
	PROJECTED_DIRECTIONS
} from './viewPlacement.js';

/** How near (paper mm) a click must be to a view's centre to pick it when the
 *  click is outside every view's box. A view drawn as four thin lines has a
 *  box a click can easily miss. */
const PICK_NEAR_MM = 20;

/** `null`, `'place-view'` or `'project-view'`. */
let mode = $state(null);
/** The place-view dialog's answers, while that mode runs. */
let pending = $state(null);
/** The parent view's id, once the projected-view tool has one. */
let parentId = $state(null);
/** What the sheet draws: `{ centreMm, extentMm, label, aligned }` or null. */
let ghost = $state(null);
/** The last probe for the running mode, keyed by what it was asked about. */
let probe = $state(null);
let probeKey = null;
/** The probe in flight, as `{ key, promise }`, so two pointer moves asking
 *  the same question wait on ONE round trip instead of racing. */
let inflight = null;
/** Pointer-move sequence, so a slow answer cannot overwrite a newer ghost. */
let moveSeq = 0;

/** The mode the sheet's pointer handlers are in (`null` when idle). */
export function placementMode() {
	return mode;
}

/** The parent view the projected-view tool is working from, or null. */
export function placementParentId() {
	return parentId;
}

/** The ghost the sheet should draw, as SVG in the sheet's own units. */
export function placementGhostSvg(sheet) {
	if (!ghost || !sheet) return '';
	return ghostSvg({
		centreMm: ghost.centreMm,
		extentMm: ghost.extentMm,
		sheetMm: sheetExtentMm(sheet),
		label: ghost.label,
		aligned: ghost.aligned
	});
}

/** Enter place-view mode with the dialog's answers. */
export function startPlaceView({ sourceTab, view, scale }) {
	reset();
	mode = 'place-view';
	pending = { sourceTab, view, scale: Number(scale) || 1 };
}

/** Enter projected-view mode, waiting for a parent view to be clicked. */
export function startProjectView() {
	reset();
	mode = 'project-view';
}

/** Leave whatever mode is running and drop the ghost. */
export function cancelPlacement() {
	reset();
}

function reset() {
	mode = null;
	pending = null;
	parentId = null;
	ghost = null;
	probe = null;
	probeKey = null;
	inflight = null;
	moveSeq += 1;
}

/**
 * The probe the module asks. Replaceable so a test can decide WHEN an answer
 * lands (`__setProbeForTest`); the ordering guarantee below is about which of
 * two answers wins, and a test that established that with a sleep would be
 * measuring the machine.
 */
let askEngine = probeDrawingView;

/**
 * Test door (D4e review): replace the probe, or restore it with no argument.
 *
 * Only `drawing-view-placement.spec.js` uses it, to land two answers in a
 * chosen order. It also drops the cache, so a test cannot inherit a real
 * answer.
 */
export function __setProbeForTest(fn) {
	askEngine = typeof fn === 'function' ? fn : probeDrawingView;
	probe = null;
	probeKey = null;
	inflight = null;
}

/**
 * Ask the engine about `projections` of `sourceTab`, once per distinct
 * question and document state.
 *
 * Cached because a hover fires per pointer move and the answer depends on the
 * document, not on the cursor — the cursor only chooses WHICH of the answers
 * to draw.
 *
 * **The key carries the document's revision** (`getDocumentRevision`), not
 * just the question. The first version of this cache keyed on the question
 * alone and argued that "any document edit leaves the mode", which was not
 * true: `reset` runs when a tool is STARTED, cancelled or completed, and
 * nothing calls it when the document changes underneath a running tool. So an
 * edit from the panel, from an agent over the relay, or from an undo left the
 * mode running with an answer describing geometry that no longer existed —
 * measured in `the_ghost_follows_an_edit_made_while_the_tool_is_running`,
 * where moving the parent view 30 mm left the ghost where the parent had
 * been. The revision moves on every `ModelUpdated`, so the stale entry is
 * simply never read again.
 *
 * A second move asking the same question while the first is in flight AWAITS
 * that round trip rather than giving up on it. Bailing out with whatever was
 * cached was this module's one real race: under load the ghost was built from
 * `null` — no box at all — and nothing recomputed it, because no further
 * pointer move was coming. Measured as a 1-in-3 flake in the GUI spec at four
 * workers.
 */
async function askProbe(question, sourceTab, projections) {
	const key = `${getDocumentRevision()}|${question}`;
	if (probeKey === key && probe) return probe;
	if (inflight && inflight.key === key) return await inflight.promise;
	const promise = askEngine(sourceTab, projections)
		.then((answer) => {
			if (answer) {
				probe = answer;
				probeKey = key;
			}
			return answer;
		})
		.finally(() => {
			if (inflight?.key === key) inflight = null;
		});
	inflight = { key, promise };
	return await promise;
}

/**
 * The sheet's pointer-move handler for the running mode. `atMm` is the paper
 * point under the cursor (`paperPointMm`).
 */
export async function placementPointerMove(atMm) {
	const sheet = getDrawingSheet(null);
	if (!mode || !sheet || !Array.isArray(atMm)) return;
	// This move's turn. A probe is a round trip, so two moves can be in the
	// air at once; the LAST one the pointer made is the one whose ghost is
	// true, and an older answer landing afterwards must not overwrite it.
	const seq = ++moveSeq;
	const stale = () => seq !== moveSeq || !mode;
	if (mode === 'place-view') {
		const answer = await askProbe(`named:${pending.sourceTab}:${pending.view}`, pending.sourceTab, [
			{ type: 'Named', view: { type: pending.view } }
		]);
		const frame = answer?.views?.[0];
		const extentMm = frame
			? ghostExtentMm({
					boundsM: answer.bounds,
					dir: frame.dir,
					up: frame.up,
					scale: pending.scale
				})
			: null;
		if (stale()) return;
		const snapped = snapPlacementMm({ atMm, views: sheet.views ?? [] });
		ghost = {
			centreMm: snapped.mm,
			extentMm,
			label: `${pending.view} of ${sourceName(pending.sourceTab)}`,
			aligned: snapped.alignedTo.some(Boolean)
		};
		return;
	}
	// Projected: before a parent is picked the cursor only highlights the view
	// it would pick; after, the sector it is in picks the direction.
	const parent = viewById(sheet, parentId);
	if (!parent) {
		const hit = viewAtMm(sheet, atMm);
		ghost = hit
			? {
					centreMm: hit.view.placement_mm ?? [0, 0],
					extentMm: hit.extentMm,
					label: `project from ${hit.view.name ?? '?'}`,
					aligned: false
				}
			: null;
		return;
	}
	const sector = sectorAtMm({
		atMm,
		centreMm: parent.placement_mm ?? [0, 0],
		extentMm: viewExtentMm(parent) ?? [0, 0]
	});
	if (!sector) {
		// Inside the parent's own box: no direction, so no ghost — rather than
		// the last one, which would read as a placement the click would make.
		ghost = null;
		return;
	}
	const sourceTab = parent.source?.tab_id;
	const answer = await askProbe(
		`projected:${parent.id}:${sourceTab}`,
		sourceTab,
		directionProjections(parent.id)
	);
	if (stale()) return;
	const index = DIRECTION_ORDER.indexOf(sector);
	const probed = answer?.views?.[index];
	if (!probed || probed.error) {
		ghost = null;
		return;
	}
	const extentMm = ghostExtentMm({
		boundsM: answer.bounds,
		dir: probed.dir,
		up: probed.up,
		scale: Number(parent.scale) || 1
	});
	ghost = {
		centreMm: probed.placement_mm,
		extentMm,
		// The engine's own label for the placement, plus the parent's name:
		// `Right of Front`, `Iso (up-right) of Front`. `shows` is the side the
		// active standard makes of it, which is the thing a first-angle
		// document changes without the tool knowing why.
		label:
			`${directionLabel(sector)} of ${parent.name ?? '?'}` +
			(probed.shows && probed.shows.type !== sector
				? ` — shows ${directionLabel(probed.shows.type).toLowerCase()}`
				: ''),
		aligned: false,
		direction: sector
	};
}

/**
 * The sheet's click handler for the running mode. Adds the view and leaves
 * the mode; answers the new view's id, or null.
 */
export async function placementPointerDown(atMm) {
	const sheet = getDrawingSheet(null);
	if (!mode || !sheet || !Array.isArray(atMm)) return null;
	if (mode === 'place-view') {
		if (!ghost) await placementPointerMove(atMm);
		// SPREAD, not the array itself: `ghost` is `$state`, so reading
		// `centreMm` hands back a reactive PROXY, and a proxy reaching
		// `bridge.send` throws `DataCloneError` — the trap recorded in the
		// 2026-09-26 notes. Measured here: the click placed nothing at all.
		const placementMm = [...(ghost?.centreMm ?? atMm)].map(Number);
		const spec = pending;
		reset();
		return await addDrawingView(spec.sourceTab, {
			view: spec.view,
			scale: spec.scale,
			placementMm
		});
	}
	const parent = viewById(sheet, parentId);
	if (!parent) {
		const hit = viewAtMm(sheet, atMm);
		if (hit) {
			parentId = hit.view.id;
			ghost = null;
			await placementPointerMove(atMm);
		}
		return null;
	}
	const direction = ghost?.direction;
	if (!direction) return null;
	const source = parent.source?.tab_id;
	const scale = Number(parent.scale) || 1;
	reset();
	// No `placementMm`: the engine places it, which is the SAME placement the
	// ghost showed (the probe answered with `default_placement`). Passing the
	// ghost's own number back would be a second copy of it travelling through
	// the page — and the one the panel does not send.
	return await addDrawingView(source, { parent: parent.id, direction, scale });
}

/** The eight projections of `parent`, in `DIRECTION_ORDER`. */
function directionProjections(parent) {
	return DIRECTION_ORDER.map((tag) => ({
		type: 'ProjectedFrom',
		parent,
		direction: { type: tag }
	}));
}

/**
 * The order the eight probe answers come back in — `PROJECTED_DIRECTIONS`'s
 * own order, which is `ProjectedDirection::ALL`'s.
 *
 * DERIVED rather than written out again (D4e review): this array indexes into
 * the probe's answers, so a third copy of the eight tags that disagreed with
 * the table by one position would label every sector with a neighbour's
 * frame — and `js_projected_direction_mirror.rs` pins the table against the
 * engine, not a literal here.
 */
const DIRECTION_ORDER = PROJECTED_DIRECTIONS.map((d) => d.tag);

function viewById(sheet, id) {
	if (!id) return null;
	return (sheet?.views ?? []).find((v) => v.id === id) ?? null;
}

/** The view a click at `atMm` picks: inside a box, else the nearest centre. */
function viewAtMm(sheet, atMm) {
	let near = null;
	for (const view of sheet?.views ?? []) {
		const centre = view.placement_mm;
		if (!Array.isArray(centre)) continue;
		const extentMm = viewExtentMm(view) ?? [0, 0];
		const dx = Math.abs(Number(atMm[0]) - Number(centre[0]));
		const dy = Math.abs(Number(atMm[1]) - Number(centre[1]));
		if (dx <= extentMm[0] / 2 && dy <= extentMm[1] / 2) return { view, extentMm };
		const d = Math.hypot(dx, dy);
		if (d <= PICK_NEAR_MM && (near === null || d < near.d)) near = { view, extentMm, d };
	}
	return near;
}

/** A source tab's name, for a ghost's label. */
function sourceName(tabId) {
	return getDocumentTabs().find((t) => t.id === tabId)?.name ?? tabId;
}
