<script>
	/**
	 * The drawing sheet (`specs/drawings_and_mbd.md` §8, D4a + D4d): the paper,
	 * with the sheet's views on it, where the 3D viewport sits on a Part tab —
	 * and, since D4d, the surface a drafter dimensions on.
	 *
	 * The PAPER is still one `{@html}` of `sheet.js`'s pure `renderSheetSvg`,
	 * for D4a's reason: the markup a user sees, the markup `export_svg` writes
	 * and the markup a byte oracle hashes are the SAME STRING. D4d adds a
	 * second, transparent `<svg>` ON TOP of it rather than reaching into that
	 * string — picking marks are not part of the drawing, must never reach an
	 * export, and are the one thing on this surface that is allowed to be
	 * declarative Svelte.
	 *
	 * ## Paper space, from the DOM
	 *
	 * Hit-testing happens in PAPER MILLIMETRES, which is what makes a 1:10 view
	 * and a 2:1 detail pick alike (`pick.js`). The conversion is taken from the
	 * rendered DOM rather than recomputed: each view is a nested
	 * `<svg class="wi-drawing">` whose user units ARE paper mm (its `viewBox`
	 * is the same size as its `width` in mm) and whose `getScreenCTM()`
	 * therefore maps paper mm to screen pixels, placement and zoom included. A
	 * second derivation of that placement — from `placement_mm`, the sheet
	 * extent and the view's own margin — would be free to disagree with where
	 * the view actually is, and then a click would bind to the edge next to the
	 * one under the cursor.
	 *
	 * It also makes the pick radius paper-constant for free: 2 mm is 2 user
	 * units in that frame at every zoom and every view scale.
	 *
	 * ## Mode dispatch
	 *
	 * One pointer path, dispatched on the store's `sheetMode`. A new tool is an
	 * arm of `onPointerDown`'s switch plus a `TOOL_FLOW` row — which is the
	 * arrangement D4e's `'place-view'` needs.
	 *
	 * It reads the store rather than taking props, the way `AssemblyPanel`
	 * does: the open Drawing tab's evaluation is store state, and a second copy
	 * passed down would be the next thing to go stale.
	 */
	import {
		getDocumentDisplayUnit,
		getDrawingAnchors,
		getDrawingSheet,
		getDrawingStatus,
		getSheetHover,
		getSheetMode,
		getSheetPicks,
		getSheetSelection,
		addSheetPick,
		addSheetAnnotation,
		clearSheetPicks,
		moveSheetAnnotation,
		setSheetHover,
		setSheetSelection
	} from '$lib/engine/store.svelte.js';
	import { renderSheetSvg, sheetExtentMm } from '$lib/drawings/sheet.js';
	import { placementForPoint } from '$lib/drawings/layout.js';
	// D4e's two placement tools. Their MODE is the store's `sheetMode` like
	// every other tool here; what the module owns is the dialog's answers, the
	// picked parent and the ghost.
	import {
		cancelPlacement,
		placementGhostSvg,
		placementMode,
		placementPointerDown,
		placementPointerMove,
		startPlaceView,
		startProjectView,
		__setProbeForTest
	} from '$lib/drawings/placementMode.svelte.js';
	import { drawingStyle } from '$lib/drawings/style.js';
	import {
		PICK_RADIUS_MM,
		TOOL_FLOW,
		anchorKindLabel,
		anchorRefusal,
		annotationGrabHandles,
		annotationSpecFor,
		pickAnnotation,
		pickableAnchors,
		placementTarget,
		resolveNearest,
		viewPaperBox,
		viewPaperTransform
	} from '$lib/drawings/pick.js';

	let { sheetId = null } = $props();

	let status = $derived(getDrawingStatus());
	let sheet = $derived(getDrawingSheet(sheetId));
	let unit = $derived(getDocumentDisplayUnit());
	/** The document's precision. M1 owns the real setting; 2 places until then. */
	const DOCUMENT_PRECISION = 2;
	let rendered = $derived(
		sheet
			? renderSheetSvg({
					sheet,
					unit,
					documentPrecision: DOCUMENT_PRECISION
				})
			: null
	);
	let mode = $derived(getSheetMode());
	let picks = $derived(getSheetPicks());
	let hover = $derived(getSheetHover());
	let selection = $derived(getSheetSelection());
	let style = drawingStyle();
	/**
	 * D4e's placement ghost, as markup for the OVERLAY.
	 *
	 * In the overlay rather than spliced into the paper string, which is where
	 * D4e first put it: the overlay already shares the sheet's `viewBox`, so
	 * its user units ARE paper millimetres and the ghost is still exact to the
	 * pixel — and D4d's rule holds, that the markup the user sees is the
	 * markup `export_svg` writes. A placement preview is not part of the
	 * drawing.
	 */
	let ghostMarkup = $derived(placementMode() ? placementGhostSvg(sheet) : '');

	/** @type {HTMLDivElement | null} */
	let stackEl = $state(null);
	/** @type {SVGSVGElement | null} */
	let overlayEl = $state(null);
	/** The drag in progress over an existing annotation, or null. */
	let drag = $state(null);
	/** Marks to draw this frame, in OVERLAY units (sheet paper mm). */
	let marks = $state({ hover: null, picks: [], selected: [], cursor: null });
	/** Where the hint balloon sits, in the stack's own pixels. */
	let hintAt = $state({ x: 0, y: 0 });
	/**
	 * The last cursor position, in CLIENT pixels. Deliberately not `$state`:
	 * the repaint effect below has to be able to re-place the cursor mark
	 * without the effect depending on anything the repaint itself writes.
	 * @type {[number, number] | null}
	 */
	let lastCursor = null;

	/** The nested view `<svg>`s, by view id. Read from the DOM each time: the
	 *  sheet's markup is replaced wholesale on every rebuild. */
	function viewElements() {
		if (!stackEl) return [];
		const out = [];
		for (const g of stackEl.querySelectorAll('g.wi-sheet-view[data-view-id]')) {
			const svg = g.querySelector('svg');
			if (svg) out.push([g.getAttribute('data-view-id'), svg]);
		}
		return out;
	}

	function viewById(viewId) {
		return (sheet?.views ?? []).find((v) => v.id === viewId) ?? null;
	}

	/** A client point in one view's paper mm, or null when the CTM is absent
	 *  (a detached or display:none element). */
	function clientToView(viewEl, clientX, clientY) {
		const m = viewEl.getScreenCTM?.();
		if (!m) return null;
		const p = new DOMPoint(clientX, clientY).matrixTransform(m.inverse());
		return Number.isFinite(p.x) && Number.isFinite(p.y) ? [p.x, p.y] : null;
	}

	/**
	 * A client point in SHEET paper millimetres measured up from the
	 * bottom-left corner — `DrawingView.placement_mm`'s own convention, which
	 * is what D4e's placement tools work in.
	 *
	 * Through the overlay's CTM, so it is the same mapping every mark on this
	 * surface is placed with; the one flip is that the overlay's user units
	 * measure y DOWN from the top, as `sheet.js` writes the paper.
	 */
	function clientToSheetMm(clientX, clientY) {
		const m = overlayEl?.getScreenCTM?.();
		if (!m || !sheet) return null;
		const p = new DOMPoint(clientX, clientY).matrixTransform(m.inverse());
		if (!Number.isFinite(p.x) || !Number.isFinite(p.y)) return null;
		const [, heightMm] = sheetExtentMm(sheet);
		return [p.x, heightMm - p.y];
	}

	/** A view-space paper point in the OVERLAY's units (the sheet's paper mm). */
	function viewToOverlay(viewEl, paper) {
		const o = overlayEl?.getScreenCTM?.();
		const v = viewEl.getScreenCTM?.();
		if (!o || !v) return null;
		const p = new DOMPoint(paper[0], paper[1]).matrixTransform(o.inverse().multiply(v));
		return Number.isFinite(p.x) && Number.isFinite(p.y) ? [p.x, p.y] : null;
	}

	/**
	 * The anchor a click at this client point would bind to.
	 *
	 * Measured per view (the cursor is a different point in each view's own
	 * frame) and decided ONCE over the merged list — two views' nearest
	 * anchors would each win locally otherwise. Every distance is a paper
	 * millimetre, which is what makes them comparable across views of
	 * different scales.
	 *
	 * @returns {{ viewId: string, viewEl: SVGSVGElement, anchor: any,
	 *             paper: [number, number], distance: number }
	 *           | { tie: any[] } | null}
	 */
	function anchorUnder(clientX, clientY) {
		const scored = [];
		for (const [viewId, viewEl] of viewElements()) {
			const cursor = clientToView(viewEl, clientX, clientY);
			if (!cursor) continue;
			const view = viewById(viewId);
			if (!view) continue;
			const tf = viewPaperTransform(view);
			for (const c of pickableAnchors(getDrawingAnchors(viewId), tf)) {
				scored.push({
					viewId,
					viewEl,
					anchor: c.anchor,
					paper: c.paper,
					distance: Math.hypot(c.paper[0] - cursor[0], c.paper[1] - cursor[1])
				});
			}
		}
		return resolveNearest(scored);
	}

	/**
	 * Whether this view's drawn annotations can be addressed by index.
	 *
	 * They can only when every authored annotation RESOLVED: the rebuild skips
	 * one it could not measure, so a view with a failure has a layout list
	 * shorter than its authored list and the two indices no longer agree. The
	 * delete and edit doors address by authored index, so selecting by layout
	 * index there would edit a different dimension — refused rather than
	 * guessed. (The fix is an index on `AnnotationLayout`, or the rebuild's
	 * `annotation_errors` indices on the wire: Rust, and an open item.)
	 */
	function indicesAgree(view) {
		return (view?.cache?.annotations?.length ?? 0) === (view?.annotations?.length ?? 0);
	}

	/** The existing annotation a click at this client point would select. */
	function annotationUnder(clientX, clientY) {
		const scored = [];
		for (const [viewId, viewEl] of viewElements()) {
			const cursor = clientToView(viewEl, clientX, clientY);
			if (!cursor) continue;
			const view = viewById(viewId);
			if (!view) continue;
			const handles = annotationGrabHandles(view, { unit, documentPrecision: DOCUMENT_PRECISION });
			const hit = pickAnnotation(handles, cursor, PICK_RADIUS_MM);
			if (hit) scored.push({ viewId, viewEl, index: hit.index, distance: hit.distance, view });
		}
		const best = resolveNearest(scored);
		// A tie between two dimensions is resolved rather than refused: both
		// are already on the paper, the user can see which they meant, and the
		// next click on a less crowded part of either selects it. The refusal
		// exists for BINDING, where the wrong choice is invisible.
		if (best && 'tie' in best) return best.tie[0];
		return best;
	}

	/** Paint the overlay from the current hover / picks / selection. */
	function refreshMarks(cursorClient) {
		const next = { hover: null, picks: [], selected: [], cursor: null };
		const h = getSheetHover();
		if (h?.viewId && h.paper) {
			const el = viewElements().find(([id]) => id === h.viewId)?.[1];
			if (el) {
				const at = viewToOverlay(el, h.paper);
				if (at) next.hover = { at, radius: h.radiusMm ?? null, label: h.label ?? '' };
			}
		}
		for (const p of getSheetPicks()) {
			const el = viewElements().find(([id]) => id === p.viewId)?.[1];
			if (!el) continue;
			const view = viewById(p.viewId);
			if (!view) continue;
			const tf = viewPaperTransform(view);
			const at = Array.isArray(p.anchor?.at) ? viewToOverlay(el, tf.toPaper(p.anchor.at)) : null;
			if (at) next.picks.push({ at, label: anchorKindLabel(p.anchor) });
		}
		const sel = getSheetSelection();
		if (sel) {
			const el = viewElements().find(([id]) => id === sel.viewId)?.[1];
			const view = viewById(sel.viewId);
			if (el && view) {
				const handles = annotationGrabHandles(view, {
					unit,
					documentPrecision: DOCUMENT_PRECISION
				});
				for (const h2 of handles.filter((x) => x.index === sel.index)) {
					for (const seg of h2.segments) {
						const a = viewToOverlay(el, seg[0]);
						const b = viewToOverlay(el, seg[1]);
						if (a && b) next.selected.push({ a, b });
					}
					for (const pt of h2.points) {
						const a = viewToOverlay(el, pt);
						if (a) next.selected.push({ a, b: a });
					}
				}
			}
		}
		if (cursorClient && overlayEl) {
			const o = overlayEl.getScreenCTM?.();
			if (o) {
				const p = new DOMPoint(cursorClient[0], cursorClient[1]).matrixTransform(o.inverse());
				if (Number.isFinite(p.x) && Number.isFinite(p.y)) next.cursor = [p.x, p.y];
			}
		}
		marks = next;
	}

	function onPointerMove(e) {
		if (!stackEl) return;
		const box = stackEl.getBoundingClientRect();
		hintAt = { x: e.clientX - box.left, y: e.clientY - box.top };
		lastCursor = [e.clientX, e.clientY];

		if (drag) {
			dragTo(e);
			return;
		}
		if (placementMode()) {
			// D4e: the cursor IS the answer in both placement tools, so there
			// is nothing to hit-test. The hover is CLEARED rather than left
			// alone — `setSheetMode` drops the picks and the selection but not
			// the hover, so a mark and a hint from the tool before this one
			// would otherwise sit under the ghost.
			if (hover) setSheetHover(null);
			const at = clientToSheetMm(e.clientX, e.clientY);
			if (at) placementPointerMove(at);
			return;
		}
		if (mode === 'select') {
			const hit = annotationUnder(e.clientX, e.clientY);
			setSheetHover(
				hit
					? {
							viewId: hit.viewId,
							index: hit.index,
							label: `dimension ${hit.index + 1}`,
							hint: indicesAgree(hit.view)
								? null
								: 'a dimension on this view did not resolve, so they cannot be selected by index'
						}
					: null
			);
			refreshMarks([e.clientX, e.clientY]);
			return;
		}
		const flow = TOOL_FLOW[mode];
		if (!flow) {
			setSheetHover(null);
			refreshMarks([e.clientX, e.clientY]);
			return;
		}
		if (picks.length >= flow.anchors) {
			// Placement phase: nothing to hit-test, the cursor IS the answer.
			setSheetHover({ hint: 'click to place this dimension' });
			refreshMarks([e.clientX, e.clientY]);
			return;
		}
		const hit = anchorUnder(e.clientX, e.clientY);
		if (!hit) {
			setSheetHover(null);
		} else if ('tie' in hit) {
			setSheetHover({
				hint:
					`${hit.tie.length} entities are equally near here (` +
					`${hit.tie.map((t) => anchorKindLabel(t.anchor)).join(', ')}) — ` +
					'move a little and pick again'
			});
		} else {
			const refusal = anchorRefusal(mode, hit.anchor);
			setSheetHover({
				viewId: hit.viewId,
				anchor: hit.anchor,
				paper: hit.paper,
				radiusMm: PICK_RADIUS_MM,
				label: anchorKindLabel(hit.anchor),
				hint: refusal
			});
		}
		refreshMarks([e.clientX, e.clientY]);
	}

	function onPointerLeave() {
		lastCursor = null;
		setSheetHover(null);
		refreshMarks(null);
	}

	async function onPointerDown(e) {
		if (e.button !== 0 || !status) return;
		if (placementMode()) {
			// D4e. The click places the view and the tool ends, so the sheet
			// swallows it: a placement click must not also reach whatever is
			// under it.
			const at = clientToSheetMm(e.clientX, e.clientY);
			if (!at) return;
			e.preventDefault();
			await placementPointerDown(at);
			return;
		}
		if (mode === 'select') {
			const hit = annotationUnder(e.clientX, e.clientY);
			if (!hit) {
				setSheetSelection(null);
				refreshMarks([e.clientX, e.clientY]);
				return;
			}
			if (!indicesAgree(hit.view)) {
				// Loud rather than selecting by an index that does not address
				// what the user clicked.
				setSheetHover({
					hint: 'a dimension on this view did not resolve, so they cannot be selected by index'
				});
				return;
			}
			setSheetSelection({ viewId: hit.viewId, index: hit.index });
			drag = {
				viewId: hit.viewId,
				index: hit.index,
				viewEl: hit.viewEl,
				moved: false,
				placement: null
			};
			capturePointer(e.pointerId, true);
			refreshMarks([e.clientX, e.clientY]);
			return;
		}
		const flow = TOOL_FLOW[mode];
		if (!flow) return;
		if (picks.length < flow.anchors) {
			await pickHere(e);
			return;
		}
		if (flow.placement) await placeHere(e);
	}

	/** One anchor pick, with every refusal named BEFORE anything is authored. */
	async function pickHere(e) {
		const hit = anchorUnder(e.clientX, e.clientY);
		if (!hit) {
			setSheetHover({ hint: `nothing within ${PICK_RADIUS_MM} mm of here` });
			return;
		}
		if ('tie' in hit) {
			setSheetHover({
				hint:
					`${hit.tie.length} entities are equally near here — ` +
					'move a little and pick again rather than binding to the wrong one'
			});
			return;
		}
		const refusal = anchorRefusal(mode, hit.anchor);
		if (refusal) {
			setSheetHover({ hint: refusal });
			return;
		}
		const added = addSheetPick(hit.viewId, hit.anchor);
		if (!added.ok) {
			setSheetHover({ hint: added.reason });
			return;
		}
		const flow = TOOL_FLOW[mode];
		if (added.picks.length === flow.anchors && !flow.placement) {
			// No placement click: the flow is complete at the last anchor.
			await author(added.picks, null);
		}
		refreshMarks([e.clientX, e.clientY]);
	}

	/** The placement click: the paper offset the drafter dropped the label at. */
	async function placeHere(e) {
		const current = getSheetPicks();
		if (current.length === 0) return;
		const viewId = current[0].viewId;
		const viewEl = viewElements().find(([id]) => id === viewId)?.[1];
		const target = viewEl ? clientToView(viewEl, e.clientX, e.clientY) : null;
		if (!target) {
			setSheetHover({ hint: 'that placement is not on the view being dimensioned' });
			return;
		}
		await author(current, placementTarget(target));
	}

	/**
	 * Author the annotation the picks describe.
	 *
	 * The placement is derived by LAYING THE ANNOTATION OUT ONCE against the
	 * picked geometry and inverting the placement rule (`placementForPoint`),
	 * which is why the anchors are turned into the layout's own
	 * `AnchorGeometry` shape here. There is no typed value anywhere on this
	 * path: the engine measures the annotation from the model and refuses a
	 * literal.
	 */
	async function author(current, target) {
		const viewId = current[0].viewId;
		const view = viewById(viewId);
		const anchors = current.map((p) => p.anchor);
		const spec = annotationSpecFor(mode, anchors, {
			text: mode === 'note' ? 'NOTE' : undefined,
			label: mode === 'datum' ? 'A' : undefined
		});
		if (!spec || !view) {
			clearSheetPicks();
			return;
		}
		if (target) {
			const placement = placementFor(view, spec, anchors, target);
			if (placement) spec.placement = placement;
		}
		await addSheetAnnotation(viewId, spec);
		refreshMarks(null);
	}

	/**
	 * The `Placement2` for a NOT-YET-AUTHORED annotation dropped at `target`.
	 *
	 * The layout rule needs resolved anchor geometry, which only the rebuild
	 * has — so this builds the one piece of it the placement arithmetic reads:
	 * a witness point per anchor, and a conic's centre and radius for a radial
	 * one. That is exactly what `ViewAnchor` carries, which is why it carries
	 * it. A kind whose placement needs more than that returns null and the
	 * annotation is authored with no placement: the automatic position is a
	 * correct drawing, and a wrong placement is a dimension sitting on the
	 * part.
	 */
	function placementFor(view, spec, anchors, target) {
		const tf = viewPaperTransform(view);
		const { centre, bounds } = viewPaperBox(view, tf);
		const probe = {
			type: spec.annotation,
			kind: spec.kind ? { type: spec.kind } : undefined,
			anchors: anchors.map((a) => anchorGeometryOf(a)),
			leader: spec.annotation === 'Note' ? anchorGeometryOf(anchors[0]) : undefined,
			anchor: spec.annotation === 'Datum' ? anchorGeometryOf(anchors[0]) : undefined
		};
		return placementForPoint(probe, style, tf, centre, bounds, target);
	}

	/**
	 * A `ViewAnchor` in the shape the layout reads an `AnchorGeometry` in.
	 *
	 * A `Line` anchor is reported as a degenerate segment at its witness point
	 * rather than as the real edge: the witness point is all `ViewAnchor`
	 * carries, and a FABRICATED pair of endpoints would give
	 * `measurementDirection` a direction the edge does not have — which for an
	 * aligned `Distance` between two parallel walls is the whole answer. So the
	 * direction falls back to the line joining the two witness points, which
	 * for the pair a drafter picks is the same direction, and the authored
	 * placement is then re-derived from the REAL geometry on the next rebuild
	 * anyway (the stored `Placement2` is an offset, not a position).
	 */
	function anchorGeometryOf(anchor) {
		const at = Array.isArray(anchor?.at) ? [Number(anchor.at[0]), Number(anchor.at[1])] : null;
		if (!at) return null;
		const shape = anchor?.shape?.type;
		const full = 2 * Math.PI;
		if (shape === 'Circle') {
			return {
				type: 'Curve',
				curve: {
					type: 'Circle',
					center: at,
					radius: Number(anchor.radius) || 0,
					start_angle: 0,
					end_angle: full
				}
			};
		}
		if (shape === 'Ellipse') {
			// The MAJOR radius is what `ViewAnchor` carries (it is the hole's
			// true radius); the minor one and the axis direction are not, and
			// the placement arithmetic reads neither — only the centre. A
			// circle is the honest stand-in rather than an invented
			// eccentricity, and the real ellipse is what the next rebuild
			// lays the placement out against.
			const r = Number(anchor.radius) || 0;
			return {
				type: 'Curve',
				curve: {
					type: 'Ellipse',
					center: at,
					major_axis: [1, 0],
					major_radius: r,
					minor_radius: r,
					start_param: 0,
					end_param: full
				}
			};
		}
		return { type: 'Point', at };
	}

	/**
	 * Take or release the pointer for the duration of a drag.
	 *
	 * Both calls THROW on a pointer the element does not have (a
	 * `NotFoundError` from `releasePointerCapture`, an `InvalidStateError` from
	 * `setPointerCapture` for a pointer that is no longer down) — which can
	 * happen for an entirely ordinary reason: the pointer left the window
	 * between the press and the release. A drag that cannot be captured still
	 * works through the element's own events, so the failure is swallowed
	 * rather than allowed to abort the commit that follows it.
	 */
	function capturePointer(pointerId, take) {
		try {
			if (take) stackEl?.setPointerCapture?.(pointerId);
			else stackEl?.releasePointerCapture?.(pointerId);
		} catch {
			/* the pointer is already gone; the drag's own events still fired */
		}
	}

	/** Live drag of a selected dimension's placement. */
	function dragTo(e) {
		const view = viewById(drag.viewId);
		const laid = view?.cache?.annotations?.[drag.index];
		const target = clientToView(drag.viewEl, e.clientX, e.clientY);
		if (!view || !laid || !target) return;
		const tf = viewPaperTransform(view);
		const { centre, bounds } = viewPaperBox(view, tf);
		const placement = placementForPoint(laid, style, tf, centre, bounds, target);
		if (placement) drag = { ...drag, moved: true, placement };
		refreshMarks([e.clientX, e.clientY]);
	}

	async function onPointerUp(e) {
		if (!drag) return;
		const d = drag;
		drag = null;
		capturePointer(e.pointerId, false);
		if (d.moved && d.placement) await moveSheetAnnotation(d.viewId, d.index, d.placement);
		refreshMarks([e.clientX, e.clientY]);
	}

	// The marks are derived from store state, so they have to be repainted when
	// it changes as well as on pointer movement — an authored dimension clears
	// the pick markers, and an undo moves the selection. It reads NOTHING the
	// repaint writes (`marks` is written here and never read), so there is no
	// cycle; the cursor comes from the non-reactive `lastCursor`.
	$effect(() => {
		void [status, mode, picks, hover, selection];
		refreshMarks(lastCursor);
	});

	/** The tool's own hint, if any — what the balloon shows. */
	let hintText = $derived(hover?.hint ?? (hover?.label ? hover.label : null));

	// A test door onto the placement module — THIS component's import of it,
	// which is the instance the pointer handlers above drive (D4e review).
	//
	// A spec that reached the module with its own `await import(...)` is not
	// reliably reaching the same instance: the app imports it through the
	// `$lib` alias and a spec imports it by path, and when the two resolve to
	// different module records the spec drives a copy whose store has no open
	// drawing — so the ghost comes out empty and the test fails for a reason
	// that has nothing to do with the code under test. Measured as exactly
	// that flake on 2026-10-05. Published here rather than on
	// `window.__waffle` because the store cannot import that module: it
	// imports the store, and the cycle would be real.
	if (typeof window !== 'undefined') {
		// @ts-ignore - test door, like `window.__waffle`
		window.__wafflePlacement = {
			startPlaceView,
			startProjectView,
			cancelPlacement,
			placementPointerMove,
			placementPointerDown,
			placementGhostSvg: () => placementGhostSvg(sheet),
			setProbeForTest: __setProbeForTest
		};
	}
</script>

<!-- The wrapper scrolls rather than overflows: an A3 sheet at 1:1 is wider
     than the viewport at most window sizes, and the page itself cannot scroll
     (CLAUDE.md, "Chrome must scroll or collapse, never overflow"). -->
<div class="drawing-sheet" data-testid="drawing-sheet" data-views={rendered?.views ?? 0}>
	{#if rendered}
		<!-- One positioned stack: the paper, then the picking overlay exactly
		     over it. The overlay shares the sheet's `viewBox`, so its user
		     units ARE paper millimetres and every mark is placed in the same
		     space the hit test works in. -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="paper-stack"
			class:placing={!!placementMode()}
			data-testid="sheet-surface"
			data-sheet-mode={mode}
			bind:this={stackEl}
			onpointermove={onPointerMove}
			onpointerdown={onPointerDown}
			onpointerup={onPointerUp}
			onpointerleave={onPointerLeave}
		>
			{@html rendered.svg}
			<svg
				class="pick-overlay"
				data-testid="sheet-overlay"
				data-hover-kind={hover?.label ?? ''}
				data-picks={picks.length}
				data-selected={selection ? selection.index : ''}
				viewBox="0 0 {rendered.widthMm} {rendered.heightMm}"
				xmlns="http://www.w3.org/2000/svg"
				bind:this={overlayEl}
				aria-hidden="true"
			>
				{#each marks.selected as s}
					<line
						class="ov-selected"
						x1={s.a[0]}
						y1={s.a[1]}
						x2={s.b[0]}
						y2={s.b[1]}
						stroke-width="1.2"
					/>
				{/each}
				{#each marks.picks as p}
					<circle class="ov-pick" cx={p.at[0]} cy={p.at[1]} r="1.4" />
				{/each}
				{#if marks.hover}
					<circle
						class="ov-hover"
						data-testid="sheet-hover-mark"
						cx={marks.hover.at[0]}
						cy={marks.hover.at[1]}
						r={marks.hover.radius ?? 1.6}
					/>
				{/if}
				{#if drag?.moved && marks.cursor}
					<circle class="ov-drag" cx={marks.cursor[0]} cy={marks.cursor[1]} r="1.2" />
				{/if}
				<!-- D4e's placement ghost. Its own units are already this
				     overlay's (paper mm, y down), so it drops straight in. -->
				{#if ghostMarkup}
					<!-- eslint-disable-next-line svelte/no-at-html-tags -->
					{@html ghostMarkup}
				{/if}
			</svg>
			{#if hintText}
				<!-- The hint follows the cursor rather than living in the
				     toolbar: a refusal is about the thing under the pointer,
				     and chrome whose width follows a message re-walks the
				     collapse ladder (see `DrawingToolbar`). -->
				<div
					class="pick-hint"
					class:refusal={!!hover?.hint}
					data-testid="sheet-hint"
					style="left: {hintAt.x}px; top: {hintAt.y}px;"
				>
					{hintText}
				</div>
			{/if}
		</div>
	{:else}
		<p class="empty" data-testid="drawing-sheet-empty">This drawing has no sheet.</p>
	{/if}
	{#if status?.errors?.length}
		<!-- A view that failed is NOT drawn, so the sheet looks finished
		     without it. The errors ride with the paper for that reason. -->
		<ul class="problems" data-testid="drawing-sheet-errors">
			{#each status.errors as e}
				<li>{e}</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.drawing-sheet {
		width: 100%;
		height: 100%;
		overflow: auto;
		background: var(--bg-secondary);
		padding: 12px;
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
	}

	/* Hugs the paper so the overlay's `inset: 0` covers exactly it. `flex`
	   rather than `block`: a block child of this centred column would stretch
	   to the column's width and the overlay would be wider than the sheet. */
	.paper-stack {
		position: relative;
		display: flex;
		max-width: 100%;
		flex: 0 0 auto;
		touch-action: none;
	}

	/* A D4e placement tool is running: the cursor says so, and the paper does
	   not select under the pointer. */
	.paper-stack.placing {
		cursor: crosshair;
		user-select: none;
	}

	/* The sheet carries its own mm size so printing is true to scale; on
	   screen it must not force the panel wider than the window. */
	.drawing-sheet :global(svg.wi-sheet) {
		max-width: 100%;
		height: auto;
		flex: 0 0 auto;
		box-shadow: 0 1px 6px rgba(0, 0, 0, 0.25);
	}

	/* Transparent, inert, and NOT part of the drawing: nothing here reaches
	   an export, which is the whole reason it is a second element rather than
	   markup injected into `renderSheetSvg`'s string. */
	.pick-overlay {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		pointer-events: none;
		overflow: visible;
	}

	.ov-hover {
		fill: none;
		stroke: var(--accent);
		stroke-width: 0.3;
	}

	.ov-pick {
		fill: var(--accent);
		stroke: none;
	}

	.ov-selected {
		stroke: var(--accent);
		stroke-opacity: 0.55;
		fill: none;
		stroke-linecap: round;
	}

	.ov-drag {
		fill: none;
		stroke: var(--accent);
		stroke-width: 0.3;
		stroke-dasharray: 1 1;
	}

	.pick-hint {
		position: absolute;
		transform: translate(12px, 14px);
		max-width: 260px;
		padding: 3px 6px;
		border-radius: 3px;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		color: var(--text-primary);
		font-size: 11px;
		line-height: 1.3;
		pointer-events: none;
		z-index: 5;
	}

	.pick-hint.refusal {
		border-color: var(--color-warning, #b80);
		color: var(--color-warning, #b80);
	}

	.empty {
		color: var(--text-secondary);
		font-size: 12px;
	}

	.problems {
		margin: 0;
		padding: 6px 10px 6px 24px;
		max-width: 100%;
		font-size: 11px;
		color: var(--color-error, #c33);
		background: var(--bg-primary);
		border-radius: 4px;
		box-sizing: border-box;
		overflow-wrap: anywhere;
	}
</style>
