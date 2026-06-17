/**
 * Smallest-region selection for overlapping shapes: two concentric circles
 * give an inner disk and an annulus. Clicking the inner disk selects the small
 * analytic region (extrudes a solid cylinder); clicking the annulus selects the
 * sub-region that extrudes a prism with a through-hole (inner walls →
 * faceRangeCount > 6).
 *
 * Before this feature the outer circle (profile 0) shadowed everything inside
 * it, so neither sub-region was reachable.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickCircle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawCircle, clickAt, moveTo } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

/** Draw two concentric circles (big r≈120px, small r≈40px) centered in canvas. */
async function sketchConcentricCircles(waffle) {
	await clickSketch(waffle.page);
	await clickCircle(waffle.page);
	await drawCircle(waffle.page, 0, 0, 120, 0); // big
	await clickCircle(waffle.page);
	await drawCircle(waffle.page, 0, 0, 40, 0); // small (snaps to shared center)
	// 1 shared center point + 2 circle entities.
	await waitForEntityCount(waffle.page, 3, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);
}

/** Open extrude, wait for regions, and clear the auto-populated region[0]. */
async function openExtrudeReadyToPick(waffle) {
	await clickExtrude(waffle.page);
	await waffle.page.waitForFunction(
		() => {
			const tree = window.__waffle.getFeatureTree();
			const sketch = tree?.features?.find((f) => f.operation?.type === 'Sketch');
			if (!sketch) return false;
			const regions = window.__waffle.getSketchRegions(sketch.id);
			return Array.isArray(regions) && regions.length >= 2;
		},
		undefined,
		{ timeout: 5000 }
	);
	// Drop the auto-populated profile 0 so the picked region drives the extrude.
	await waffle.page.evaluate(() => {
		const regions = window.__waffle.getExtrudeRegions();
		for (let i = regions.length - 1; i >= 0; i--) window.__waffle.removeExtrudeRegion(i);
	});
}

/** Click a viewport offset to pick the region under it. */
async function pickAt(waffle, px, py) {
	await moveTo(waffle.page, px, py);
	await waffle.page.waitForTimeout(120);
	await clickAt(waffle.page, px, py);
}

/** The most-recently-picked sketch region in the extrude dialog. */
async function lastSketchRegion(waffle) {
	return waffle.page.evaluate(() => {
		const regions = window.__waffle.getExtrudeRegions().filter((r) => r.type === 'sketchProfile');
		return regions[regions.length - 1] ?? null;
	});
}

test('clicking the inner disk selects the small analytic region → solid cylinder', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);

	await sketchConcentricCircles(waffle);
	await openExtrudeReadyToPick(waffle);

	// The center is inside only the inner disk (the smallest region there).
	await pickAt(waffle, 0, 0);
	const picked = await lastSketchRegion(waffle);
	expect(picked, 'a sketch region was picked').not.toBeNull();
	// Inner disk coincides with a whole circle → analytic provenance (no holes).
	expect(picked.region?.profile_entity_ids ?? null).not.toBeNull();
	expect((picked.region?.holes ?? []).length).toBe(0);

	// Extrude the selection (apply path verified end-to-end by the annulus test).
	await waffle.page.evaluate((pi) => window.__waffle.applyExtrude(0.02, pi, false, {}), picked.profileIndex);
	await waitForFeatureCount(waffle.page, 2, 10000);

	const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());
	expect(meshes.length).toBeGreaterThanOrEqual(1);
	// A solid disk has no inner walls — far fewer faces than a holed prism.
	expect(meshes[0].faceRangeCount).toBeLessThanOrEqual(6);
	expectNoAnyCrash(crashes);
});

test('clicking the annulus extrudes a prism with a through-hole (KV-region)', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);

	await sketchConcentricCircles(waffle);
	await openExtrudeReadyToPick(waffle);

	// Between the two circles → the annulus sub-region (no whole-loop profile).
	await pickAt(waffle, 60, 0);
	const picked = await lastSketchRegion(waffle);
	expect(picked, 'a sketch region was picked').not.toBeNull();
	// Annulus is a genuine sub-region: a hole, and no analytic provenance.
	expect(picked.region?.profile_entity_ids ?? null).toBeNull();
	expect((picked.region?.holes ?? []).length).toBe(1);

	// The ghost preview must reflect the picked region (a hole), not the parent
	// disk — i.e. preview matches what will extrude.
	const preview = await waffle.page.evaluate(() => window.__waffle.getExtrudePreviewParams());
	expect(preview?.[0]?.region?.holes?.length, 'preview carries the region hole').toBe(1);

	await waffle.page.locator('[data-testid="extrude-depth"]').fill('20');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);

	const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());
	expect(meshes.length).toBeGreaterThanOrEqual(1);
	// TRUE CURVES: the annulus extrudes as exact cylinder walls (a few patches
	// per circle) + caps — inner walls present (>6) but NOT a faceted prism
	// (~140 faces at this tessellation).
	expect(
		meshes[0].faceRangeCount,
		`annulus should be a holed solid with cylinder walls, got ${meshes[0].faceRangeCount}`
	).toBeGreaterThan(6);
	expect(meshes[0].faceRangeCount).toBeLessThanOrEqual(24);
	expectNoAnyCrash(crashes);
});
