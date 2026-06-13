/**
 * KV14 — a holed region drawn as ONE sketch (outer rectangle + inner rectangle)
 * extrudes as an annulus (the inner loop becomes a hole, not a separate solid).
 *
 * Discriminator: an annular prism has the hole's inner walls (≥ 8 faces) vs a
 * plain box's 6 — so faceRangeCount > 6 confirms the hole assembled.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

test('a sketch with an inner loop extrudes as an annulus (KV14)', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);

	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -120, -120, 120, 120);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -40, -40, 40, 40);
	await waitForEntityCount(waffle.page, 16, 5000); // two rects × 8 entities

	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);

	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('20');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);

	const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());
	expect(meshes.length).toBeGreaterThanOrEqual(1);
	const faces = meshes[0].faceRangeCount;
	expect(faces, `annulus should have inner walls (>6 faces), got ${faces}`).toBeGreaterThan(6);
	expectNoAnyCrash(crashes);
});
