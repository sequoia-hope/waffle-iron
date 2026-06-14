/**
 * KV13 F6c — inverse: select a feature → highlight its faces.
 *
 * The viewport colours every face whose CREATING feature is the one selected
 * in the tree (`created_by_feature === selectedFeatureId`). This asserts the
 * inverse wiring deterministically: selecting the first extrude makes its
 * introduced faces (which survive into the merged body) identifiable, and the
 * recompute runs without crashing. (Pixel-exact material colour is not
 * asserted; the data the highlight keys on is.)
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

test('selecting a feature highlights its introduced faces (KV13 F6c)', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);

	await extrudeRect(waffle.page, -80, -60, 80, 60, 20, 2); // extrude e1 = feature 2
	await extrudeRect(waffle.page, -30, -30, 30, 30, 40, 4); // extrude e2 = feature 4 (auto-union)

	const e1 = await waffle.page.evaluate(
		() => window.__waffle.getFeatureTree().features[1].id
	);

	// Select the FIRST extrude in the feature tree (feature-item-1).
	await waffle.page.locator('[data-testid="feature-item-1"]').click();

	// The selection drives the viewport highlight.
	const selected = await waffle.page.evaluate(() => window.__waffle.getSelectedFeatureId());
	expect(selected).toBe(e1);

	// Its introduced faces are present in the (merged) body — the set the
	// viewport colours green. (Phase D would have NONE of the merged body's
	// faces attributed to e1.)
	const e1FaceCount = await waffle.page.evaluate((fid) => {
		let n = 0;
		for (const m of window.__waffle.getMeshes())
			for (const r of m.faceRanges || []) if (r.created_by_feature === fid) n++;
		return n;
	}, e1);
	expect(e1FaceCount).toBeGreaterThan(0);

	expectNoAnyCrash(crashes);
});
