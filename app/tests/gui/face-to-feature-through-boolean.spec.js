/**
 * KV13 F6b — face → INTRODUCING feature, through a boolean.
 *
 * Two overlapping extrudes auto-union into one body. Phase-D Tier 1 would
 * report every face of that body as belonging to the LAST feature (the second
 * extrude / the union). F6b resolves each face to the feature that *introduced*
 * its geometry via the kernel persistent-id lineage: the merged body therefore
 * carries faces attributed to BOTH extrudes. This asserts that through-boolean
 * attribution reaches the app (the `created_by_feature` on each face range).
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

async function extrudeRect(page, x0, y0, x1, y1, depth, featureCount) {
	await clickSketch(page);
	await clickRectangle(page);
	await drawRectangle(page, x0, y0, x1, y1);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, featureCount - 1, 10000);
	await clickExtrude(page);
	await page.locator('[data-testid="extrude-depth"]').fill(String(depth));
	await page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(page, featureCount, 10000);
}

test('a merged body attributes faces to both introducing extrudes (KV13 F6b)', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);

	// Base box (extrude e1), then an overlapping boss (extrude e2) that
	// auto-unions into it.
	await extrudeRect(waffle.page, -80, -60, 80, 60, 20, 2); // sketch=1, extrude e1=2
	await extrudeRect(waffle.page, -30, -30, 30, 30, 40, 4); // sketch=3, extrude e2=4

	const ids = await waffle.page.evaluate(() =>
		window.__waffle.getFeatureTree().features.map((f) => f.id)
	);
	const e1 = ids[1];
	const e2 = ids[3];

	const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());

	// Global: faces resolve to BOTH original extrudes (not collapsed to the
	// last feature, which is what Phase-D Tier 1 would have done).
	const allCreatedBy = new Set();
	let mixedBody = false;
	for (const m of meshes) {
		const perMesh = new Set();
		for (const r of m.faceRanges || []) {
			if (r.created_by_feature) {
				allCreatedBy.add(r.created_by_feature);
				perMesh.add(r.created_by_feature);
			}
		}
		// A single rendered body whose faces trace to >1 introducing feature is
		// the through-boolean signature.
		if (perMesh.size >= 2) mixedBody = true;
	}

	expect(allCreatedBy.has(e1), 'some face traces to the first extrude').toBe(true);
	expect(allCreatedBy.has(e2), 'some face traces to the second extrude').toBe(true);
	expect(mixedBody, 'a merged body carries faces from both extrudes').toBe(true);

	expectNoAnyCrash(crashes);
});
