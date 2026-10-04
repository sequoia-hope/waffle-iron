/**
 * The pick radius is a PAPER distance (`specs/drawings_and_mbd.md` §8, D4d).
 *
 * This is the increment's one property, and the reason the hit test works in
 * the nested view `<svg>`'s own user units: 2 mm has to mean 2 mm on the paper
 * at every zoom and every view scale. A model-space radius would make a 1:10
 * view ten times more forgiving than a 2:1 detail about the same gesture, and a
 * pixel radius would make the whole thing depend on the window width.
 *
 * ## How it is measured
 *
 * For one isolated anchor, hover at a paper offset just INSIDE the radius and
 * just OUTSIDE it, and assert the highlight appears and does not. The offset
 * is converted to pixels with the rendered zoom (the view `<svg>`'s own screen
 * CTM), which is the only thing between a paper millimetre and a pixel — so
 * the conversion is the app's own placement, read back, and the RADIUS is what
 * is under test.
 *
 * Then do it again with the zoom changed (a narrower window scales the A3 sheet
 * down) and with the view scale changed (1:1 and 1:2). The zoom assertion is
 * only meaningful if the zoom actually moved, so the two pixel-per-millimetre
 * figures are asserted to differ.
 *
 * The probe direction is OUTWARD from the view's centre through the anchor, so
 * stepping further never walks toward another anchor and turns "outside the
 * radius" into "inside a different one's".
 */
import { test, expect } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { clickTool } from './helpers/toolbar.js';
import { anchorScreenPoints, plateAndDrawing, wallPair } from './helpers/drawing.js';

let crashes = null;
test.beforeEach(({ waffle }) => {
	crashes = collectCrashErrors(waffle.page);
});
test.afterEach(() => {
	const tracker = crashes;
	crashes = null;
	if (tracker) expectNoAnyCrash(tracker);
});

/** The pick radius `pick.js` declares, in paper mm. */
const RADIUS_MM = 2;
/** How far inside / outside to probe. Comfortably over any CTM rounding. */
const MARGIN_MM = 0.6;

/**
 * Probe one anchor at `RADIUS_MM ∓ MARGIN_MM` along the outward direction and
 * answer `{ inside, outside, pxPerMm }` — whether the sheet highlighted an
 * anchor at each.
 */
async function probe(page, viewId) {
	const points = await anchorScreenPoints(page, viewId);
	expect(points, 'the view reports anchors with screen positions').toBeTruthy();
	const pair = wallPair(points);
	expect(pair, 'the view offers a wall to probe').toBeTruthy();
	const anchor = pair[0];
	// Outward from the view centre, in PAPER space, then into pixels. A unit
	// paper direction scales to pixels by the same factor in x and y: the
	// sheet's own transform is a uniform scale (`max-width: 100%` on an svg
	// with a `viewBox`), so one `pxPerMm` is the whole conversion.
	const dx = anchor.paper[0] - points.centrePaper[0];
	const dy = anchor.paper[1] - points.centrePaper[1];
	const len = Math.hypot(dx, dy);
	expect(len, 'the anchor is off the view centre, so there is an outward direction').toBeGreaterThan(
		0.1
	);
	const ux = dx / len;
	const uy = dy / len;
	const at = (mm) => ({
		x: anchor.x + ux * mm * points.pxPerMm,
		y: anchor.y + uy * mm * points.pxPerMm
	});

	const inside = at(RADIUS_MM - MARGIN_MM);
	await page.mouse.move(inside.x, inside.y);
	const hitInside = await page.evaluate(() => window.__waffle.getSheetHover()?.anchor?.pid ?? null);

	const outside = at(RADIUS_MM + MARGIN_MM);
	await page.mouse.move(outside.x, outside.y);
	const hitOutside = await page.evaluate(() => window.__waffle.getSheetHover()?.anchor?.pid ?? null);

	return {
		pxPerMm: points.pxPerMm,
		pid: anchor.pid,
		inside: hitInside,
		outside: hitOutside
	};
}

test.describe('The pick radius is a paper distance (D4d)', () => {
	test('it is the same 2 mm of paper at two zooms', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		await clickTool(page, 'dim-distance');

		await page.setViewportSize({ width: 1600, height: 760 });
		const wide = await probe(page, viewId);
		await page.setViewportSize({ width: 900, height: 760 });
		const narrow = await probe(page, viewId);

		// The zoom really did change — otherwise the two probes are one probe.
		// The A3 sheet is `max-width: 100%`, so a narrower window scales the
		// whole paper down and a paper millimetre is fewer pixels.
		expect(narrow.pxPerMm, 'the narrower window renders the sheet smaller').toBeLessThan(
			wide.pxPerMm - 0.05
		);

		for (const [name, r] of [
			['wide', wide],
			['narrow', narrow]
		]) {
			expect(r.inside, `${name}: 1.4 mm of paper away is inside the radius`).toBe(r.pid);
			expect(r.outside, `${name}: 2.6 mm of paper away is outside it`).toBeNull();
		}
	});

	test('it is the same 2 mm of paper at two view scales', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		await clickTool(page, 'dim-distance');

		const full = await probe(page, viewId);
		await page.evaluate((id) => window.__waffle.editDrawingView(id, { scale: 0.5 }), viewId);
		await page.waitForFunction(
			(id) => {
				const sheets = window.__waffle?.getDrawingStatus()?.drawing?.sheets ?? [];
				return (sheets.flatMap((s) => s.views ?? []).find((v) => v.id === id)?.scale ?? 0) === 0.5;
			},
			viewId,
			{ timeout: 10000 }
		);
		const half = await probe(page, viewId);

		// At 1:2 the SAME 2 mm of paper is 4 mm of model, so a radius measured
		// in model units would have halved on paper and the inside probe would
		// have missed. It does not.
		for (const [name, r] of [
			['1:1', full],
			['1:2', half]
		]) {
			expect(r.inside, `${name}: 1.4 mm of paper away is inside the radius`).toBe(r.pid);
			expect(r.outside, `${name}: 2.6 mm of paper away is outside it`).toBeNull();
		}
		// And the pid is the same entity in both: the view was re-scaled, not
		// re-projected onto different geometry.
		expect(half.pid).toBe(full.pid);
	});
});
