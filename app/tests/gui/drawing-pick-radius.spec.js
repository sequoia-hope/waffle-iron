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
 * is under test. Each hover is WAITED for rather than sampled, because a
 * sample cannot tell a point outside the radius from a pointermove the page
 * has not handled yet (see `waitForHover`).
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
import {
	anchorScreenPoints,
	authoredAnnotations,
	plateAndDrawing,
	waitForAnnotationCount,
	wallPair
} from './helpers/drawing.js';

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
 * Wait until the sheet's hover state is `pid` (or null), and FAIL if it never
 * is.
 *
 * Every probe below is a TRANSITION, which is the only form that can be
 * waited on without swallowing the answer. Reading `getSheetHover()` straight
 * after a `mouse.move` cannot tell "the radius excluded this point" from "the
 * pointermove handler has not run yet", and under heavy load on this box the
 * second happens — measured as two spurious failures in a run at load average
 * 133 on 24 cores, in tests that pass at load 61. So the probe moves between
 * states whose expected value differs from the current one, and this wait is
 * the assertion: a radius that is wrong in either direction times out here
 * rather than being read as a stale value.
 */
async function waitForHover(page, pid, what) {
	await page
		.waitForFunction(
			(want) => (window.__waffle.getSheetHover()?.anchor?.pid ?? null) === want,
			pid,
			{ timeout: 10000 }
		)
		.catch(() => {
			throw new Error(
				`${what}: the sheet's hover never became ${pid === null ? 'nothing' : pid}`
			);
		});
}

/**
 * Probe one anchor at `RADIUS_MM ∓ MARGIN_MM` along the outward direction,
 * asserting as it goes, and answer `{ pxPerMm, pid }` — the zoom it probed at
 * and the entity it probed against, for the caller to compare across runs.
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

	// Three moves, each a transition from the state before it, so each wait is
	// an assertion rather than a sample. On the anchor first: it establishes a
	// known hover AND proves the anchor is reachable at all, which is what
	// makes the two probes after it mean what they say.
	await page.mouse.move(anchor.x, anchor.y);
	await waitForHover(page, anchor.pid, 'on the anchor');

	const outside = at(RADIUS_MM + MARGIN_MM);
	await page.mouse.move(outside.x, outside.y);
	await waitForHover(page, null, `${RADIUS_MM + MARGIN_MM} mm of paper away`);

	const inside = at(RADIUS_MM - MARGIN_MM);
	await page.mouse.move(inside.x, inside.y);
	await waitForHover(page, anchor.pid, `${RADIUS_MM - MARGIN_MM} mm of paper away`);

	// Nothing to assert at the call site: the three waits above ARE the
	// assertion, and returning the outcomes for the caller to re-check would
	// only restate values this function already proved.
	return { pxPerMm: points.pxPerMm, pid: anchor.pid };
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

		// Both probes already asserted the radius at their own zoom, inside
		// `probe`. What is left to say here is that they probed the same
		// entity, so the two runs are comparable at all.
		expect(narrow.pid).toBe(wide.pid);
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
		// in model units would have halved on paper and `probe`'s inside wait
		// would have timed out at 1:2. It does not. And the pid is the same
		// entity in both: the view was re-scaled, not re-projected onto
		// different geometry.
		expect(half.pid).toBe(full.pid);
	});

	test('a window resize BETWEEN the two picks does not move the anchors', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		const { viewId } = await plateAndDrawing(page);
		await page.setViewportSize({ width: 1600, height: 760 });

		const wide = await anchorScreenPoints(page, viewId);
		const pairWide = wallPair(wide);
		expect(pairWide, 'the view offers a wall pair at the wide size').toBeTruthy();

		await clickTool(page, 'dim-distance');
		// First pick at the wide size.
		await page.mouse.move(pairWide[0].x, pairWide[0].y);
		await page.mouse.down();
		await page.mouse.up();
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '1');

		// The paper is re-scaled under a half-finished pick. Every screen
		// position the flow has yet to use is now somewhere else, which is why
		// the hit test reads its transform from the DOM on every event rather
		// than caching one: a cached CTM would bind the second anchor to
		// whatever entity now sits where the old pixels pointed.
		await page.setViewportSize({ width: 900, height: 760 });
		const narrow = await anchorScreenPoints(page, viewId);
		expect(narrow.pxPerMm, 'the resize really did change the zoom').toBeLessThan(
			wide.pxPerMm - 0.05
		);
		const second = narrow.anchors.find((a) => a.pid === pairWide[1].pid);
		expect(second, 'the second wall is still in the anchor list').toBeTruthy();

		await page.mouse.move(second.x, second.y);
		await page.mouse.down();
		await page.mouse.up();
		await expect(page.getByTestId('toolbar')).toHaveAttribute('data-sheet-picks', '2');
		await page.mouse.move(second.x, second.y - 10 * narrow.pxPerMm);
		await page.mouse.down();
		await page.mouse.up();

		await waitForAnnotationCount(page, viewId, 1);
		const named = (await authoredAnnotations(page, viewId))[0].anchors.map((r) => r.selector?.pid);
		expect(new Set(named)).toEqual(new Set([pairWide[0].pid, pairWide[1].pid]));
	});
});

/**
 * The same property at a device scale factor of 2.
 *
 * `getScreenCTM()` maps user units to CSS pixels, not to device pixels, so a
 * retina display must change nothing here — and the whole conversion going
 * through that one matrix is what makes that true for free. Pinned because
 * the plausible wrong version (multiplying by `devicePixelRatio` anywhere on
 * the path) passes every other test in this file.
 */
test.describe('The pick radius at devicePixelRatio 2 (D4d)', () => {
	test.use({ deviceScaleFactor: 2 });

	test('2 mm of paper is still 2 mm of paper', async ({ waffle }) => {
		const page = waffle.page;
		await waffle.waitForReady();
		expect(await page.evaluate(() => window.devicePixelRatio)).toBe(2);
		const { viewId } = await plateAndDrawing(page);
		await clickTool(page, 'dim-distance');
		// `probe` asserts the radius itself, inside and outside, by waiting
		// for each transition.
		const r = await probe(page, viewId);
		expect(r.pid, 'the probe found a wall to measure against').toBeTruthy();
	});
});
