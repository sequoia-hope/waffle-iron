/**
 * Drawing-tab helpers for the GUI suite (D4a–D4d).
 *
 * The fixture (a plate, a drawing tab, a top view of it) and the two
 * coordinate conversions a dimensioning spec needs.
 *
 * ## The conversions are DERIVED here, on purpose
 *
 * `paperOf` recomputes the view-space → paper-mm map from the view's own
 * `bbox` and `scale` rather than asking the app for it, and reads the screen
 * placement from the rendered `<svg>`'s `getScreenCTM()`. That makes the spec
 * PREDICT where an anchor is from the layout record instead of recording
 * whatever the app's own picking module says — which is what lets it fail when
 * the app's map and the record disagree.
 */
import { expect } from '@playwright/test';

/** The D3/D4a fixture plate, in METERS (the document's own unit). */
export const PLATE_W = 0.04;
export const PLATE_D = 0.025;
export const PLATE_T = 0.01;
/** The same extents in millimetres, which is what the sheet prints. */
export const PLATE_W_MM = 40;
export const PLATE_D_MM = 25;

/** The document's first Part tab, read AFTER an engine round trip. */
export async function partTabId(page) {
	return page.evaluate(() => {
		const tabs = window.__waffle.getDocumentState().documentTabs ?? [];
		return (tabs.find((t) => t.kind === 'Part') ?? tabs[0])?.id ?? null;
	});
}

/**
 * Build the plate on the active Part tab, then a Drawing tab with a top view
 * of it. Returns `{ partTab, drawingTab, viewId }`.
 *
 * The sketch is a FIXTURE, which is the one use of `addSketchEntity` CLAUDE.md
 * allows; the dimensioning the specs are about is real pointer events.
 */
export async function plateAndDrawing(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(() => window.__waffle?.getState()?.sketchMode?.active === true, null, {
		timeout: 5000
	});
	await page.evaluate(
		([w, d]) => {
			const waffle = window.__waffle;
			for (const [id, x, y] of [
				[1, 0, 0],
				[2, w, 0],
				[3, w, d],
				[4, 0, d]
			]) {
				waffle.addSketchEntity({ type: 'Point', id, x, y, construction: false });
			}
			for (const [id, a, b] of [
				[10, 1, 2],
				[11, 2, 3],
				[12, 3, 4],
				[13, 4, 1]
			]) {
				waffle.addSketchEntity({ type: 'Line', id, start_id: a, end_id: b, construction: false });
			}
		},
		[PLATE_W, PLATE_D]
	);
	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1,
		null,
		{ timeout: 10000 }
	);
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.evaluate((t) => window.__waffle.applyExtrude(t, 0, false), PLATE_T);
	await page.waitForFunction(
		() => (window.__waffle?.getMeshes() ?? []).some((m) => m.triangleCount > 0),
		null,
		{ timeout: 20000 }
	);

	const partTab = await partTabId(page);
	expect(partTab, 'the document has a Part tab to draw').toBeTruthy();
	const drawingTab = await page.evaluate(() => window.__waffle.addTab('Drawing'));
	expect(drawingTab, 'the engine added a Drawing tab').toBeTruthy();
	await page.evaluate((id) => window.__waffle.switchTab(id), drawingTab);
	await page.waitForFunction(() => window.__waffle?.getDrawingStatus() !== null, null, {
		timeout: 15000
	});
	const viewId = await addView(page, partTab, { view: 'Top' });
	return { partTab, drawingTab, viewId };
}

/** Add a view and wait for it to have drawn something. */
export async function addView(page, partTab, options) {
	const viewId = await page.evaluate(
		([tab, opts]) => window.__waffle.addDrawingView(tab, opts),
		[partTab, options]
	);
	expect(viewId, 'the view was added').toBeTruthy();
	await page.waitForFunction(
		(id) => {
			const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
			const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
			return (view?.cache?.curves?.length ?? 0) > 0;
		},
		viewId,
		{ timeout: 20000 }
	);
	return viewId;
}

/**
 * One view's pickable anchors, with their paper-mm points and their screen
 * positions — everything a pointer gesture needs.
 *
 * `pxPerMm` is the rendered zoom: the `a` term of the view `<svg>`'s screen
 * CTM, which is the only thing between a paper millimetre and a pixel (the
 * nested svg's user units ARE paper mm). A spec that wants to probe "2.5 mm
 * away" multiplies by it.
 */
export async function anchorScreenPoints(page, viewId) {
	return page.evaluate((id) => {
		const st = window.__waffle.getDrawingStatus();
		const view = (st?.drawing?.sheets ?? []).flatMap((s) => s.views ?? []).find((v) => v.id === id);
		const anchors = (st?.anchors ?? {})[id] ?? [];
		const g = document.querySelector(`g.wi-sheet-view[data-view-id="${id}"]`);
		const svg = g?.querySelector('svg');
		const m = svg?.getScreenCTM?.();
		if (!view?.cache?.bbox || !m) return null;
		// Derived here rather than imported: the spec predicts where the anchor
		// is from the LAYOUT RECORD, so it can disagree with the app.
		const [[minU], [, maxV]] = view.cache.bbox;
		const mmPerMeter = 1000 * (view.scale || 1);
		const out = [];
		for (const a of anchors) {
			if (!Array.isArray(a.at)) continue;
			const paper = [(a.at[0] - minU) * mmPerMeter, -(a.at[1] - maxV) * mmPerMeter];
			const p = new DOMPoint(paper[0], paper[1]).matrixTransform(m);
			out.push({
				pid: String(a.pid),
				shape: a.shape?.type ?? '?',
				kind: a.kind?.type ?? '?',
				at: a.at,
				paper,
				x: p.x,
				y: p.y
			});
		}
		return { pxPerMm: m.a, anchors: out, centrePaper: viewCentre(view, mmPerMeter) };

		function viewCentre(v, mmpm) {
			const [[u0, v0], [u1, v1]] = v.cache.bbox;
			return [((u1 - u0) * mmpm) / 2, ((v1 - v0) * mmpm) / 2];
		}
	}, viewId);
}

/** The two opposite WALL anchors a width dimension measures between. */
export function wallPair(points) {
	const walls = points.anchors.filter((a) => a.shape === 'Line');
	for (const a of walls) {
		for (const b of walls) {
			if (a.pid === b.pid) continue;
			if (Math.abs(a.at[1] - b.at[1]) > 1e-12) continue;
			if (Math.abs(b.at[0] - a.at[0]) > 1e-9) return [a, b];
		}
	}
	return null;
}

/**
 * The OTHER wall pair — the two whose witness points share an `u` — so a spec
 * can author a SECOND dimension anchored on different entities.
 *
 * A second dimension is what makes an index bug visible: with one annotation
 * on a view, every index is 0 and an inverse that addresses the wrong one
 * addresses the right one anyway.
 */
export function crossWallPair(points) {
	const walls = points.anchors.filter((a) => a.shape === 'Line');
	for (const a of walls) {
		for (const b of walls) {
			if (a.pid === b.pid) continue;
			if (Math.abs(a.at[0] - b.at[0]) > 1e-12) continue;
			if (Math.abs(b.at[1] - a.at[1]) > 1e-9) return [a, b];
		}
	}
	return null;
}

/** One annotation's anchor pids, sorted — its identity for a comparison. */
export function anchorPids(annotation) {
	return (annotation?.anchors ?? []).map((r) => r.selector?.pid).sort();
}

/** Author a `Distance` dimension between `pair`, placed `offsetMm` away. */
export async function dimensionPair(page, viewId, points, pair, offset) {
	const { clickTool } = await import('./toolbar.js');
	await clickTool(page, 'dim-distance');
	await clickAt(page, pair[0].x, pair[0].y);
	await clickAt(page, pair[1].x, pair[1].y);
	await clickAt(
		page,
		pair[0].x + (offset?.x ?? 0) * points.pxPerMm,
		pair[0].y + (offset?.y ?? -10) * points.pxPerMm
	);
}

/** Click at a client point with a real move first, so hover runs. */
export async function clickAt(page, x, y) {
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.up();
}

/** The authored annotations of one view (anchors and placement included). */
export async function authoredAnnotations(page, viewId) {
	return page.evaluate((id) => {
		const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
		const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
		return view?.annotations ?? [];
	}, viewId);
}

/** Wait until the view carries `n` authored annotations. */
export async function waitForAnnotationCount(page, viewId, n, timeout = 20000) {
	await page.waitForFunction(
		([id, want]) => {
			const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
			const view = sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id);
			return (view?.annotations?.length ?? 0) === want;
		},
		[viewId, n],
		{ timeout }
	);
}
