/**
 * KV12 Tier 2 (E4) — extruding a closed profile that contains an ARC, with
 * EXACT cylinder side patches.
 *
 * A closed loop with an arc edge (a D-shape: diameter line + semicircle arc)
 * now reconstructs to an exact line/arc loop: the arc becomes cylinder side
 * patches instead of one planar wall per chord sample. So the body has only a
 * handful of faces (2 caps + 1 diameter wall + a few cylinder patches), not
 * the ~18 a chord polygon yields. (The 180° arc exceeds the kernel's
 * minor-arc limit, so it splits into < π sub-arcs ⇒ several cylinder patches.)
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

test('a closed profile with an arc edge extrudes with cylinder walls (KV12 Tier 2)', async ({ waffle }) => {
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

	// Tier 2: the arc is swept as exact cylinder patches, so the body has only
	// a handful of B-rep faces. A Tier-1 chord polygon would tessellate the
	// semicircle into ~16 separate planar walls (~18 faces total); the exact
	// path keeps it well under 10. This assertion fails if arc extrude
	// regresses to the chord-polygon approximation.
	const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());
	const faceRangeCount = meshes.reduce((m, x) => Math.max(m, x.faceRangeCount), 0);
	expect(faceRangeCount).toBeGreaterThan(3); // caps + walls exist
	expect(faceRangeCount).toBeLessThanOrEqual(10); // not one wall per chord sample

	expectNoAnyCrash(crashes);
});
