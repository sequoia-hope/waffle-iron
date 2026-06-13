/**
 * KV12 Tier 1 — extruding a closed profile that contains an ARC.
 *
 * Regression for "make_faces_from_profiles: arc-segment profile (curved
 * geometry not yet in kernel-v2)". A closed loop with an arc edge (a D-shape:
 * diameter line + semicircle arc) now extrudes via its sampled chord polygon.
 *
 * Real pointer drawing, real engine path (FinishSketch → make_faces → extrude).
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickLine,
	clickArc,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawLine, drawArc } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	hasMeshWithGeometry,
	waitForFeatureCount,
} from './helpers/state.js';

test('a closed profile with an arc edge extrudes (KV12 Tier 1)', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);

	await clickSketch(waffle.page);

	// Diameter line, then a semicircle arc back to the start — endpoints
	// coincide (auto-reused), closing a D-shaped profile (line + arc).
	await clickLine(waffle.page);
	await drawLine(waffle.page, -60, 0, 60, 0);
	await clickArc(waffle.page);
	await drawArc(waffle.page, 0, 0, 60, 0, -60, 0);

	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);

	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('20');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);

	// The extrude must produce real geometry (no NotSupported error toast,
	// no crash) — this is the capability that was previously walled.
	expect(await hasMeshWithGeometry(waffle.page)).toBe(true);
	await expect(waffle.page.locator('.body-item')).toHaveCount(1);
	expectNoAnyCrash(crashes);
});
