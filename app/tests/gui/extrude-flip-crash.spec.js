/**
 * Extrude Flip Direction — No WASM Crash
 *
 * Verifies that extruding a sketch in opposite directions does not crash
 * the WASM engine. With panic=unwind enabled (via nightly + -Zbuild-std),
 * catch_unwind in the boolean cascade catches truck panics and returns
 * a graceful error instead of killing the module.
 *
 * Uses console error monitoring for crash detection — NOT engineReady,
 * which is unreliable (see docs/TESTING.md "WASM Crash Detection").
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	getFeatureCount,
	getMeshes,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

/**
 * Create a sketch with a rectangle via __waffle API (fast, deterministic).
 */
async function createSketchWithRectangle(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: -30, y: -30 });
		w.addSketchEntity({ type: 'Point', id: 2, x: 30, y: -30 });
		w.addSketchEntity({ type: 'Point', id: 3, x: 30, y: 30 });
		w.addSketchEntity({ type: 'Point', id: 4, x: -30, y: 30 });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	});
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, 1, 10000);
	await page.waitForTimeout(200);
}

/**
 * Apply an extrude via __waffle API with given depth and options.
 */
async function applyExtrude(page, depth, opts = {}) {
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(
		({ depth, opts }) => window.__waffle.applyExtrude(depth, 0, false, opts),
		{ depth, opts }
	);
}

test.describe('Extrude flip direction — no crash', () => {
	test('extrude +Z then -Z does not crash WASM engine', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		// Step 1: Create sketch with rectangle
		await createSketchWithRectangle(page);

		// Step 2: Extrude forward (+Z, depth=10)
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		const meshesAfterFirst = await getMeshes(page);
		expect(meshesAfterFirst.some(m => m.triangleCount > 0)).toBe(true);

		// Step 3: Extrude same sketch in opposite direction (-Z)
		// This triggers auto-union with fully coplanar shared face.
		// catch_unwind catches the panic and returns an error — no crash.
		await applyExtrude(page, 10, { flipDirection: true });
		await page.waitForTimeout(5000);

		// Step 4: ZERO crashes — catch_unwind handles it
		expectNoAnyCrash(crashTracker);

		// Step 5: Verify engine is fully functional (not just alive)
		const featureCount = await getFeatureCount(page);
		expect(featureCount).toBeGreaterThanOrEqual(2);

		// First extrude mesh should still be intact
		const meshesAfter = await getMeshes(page);
		expect(meshesAfter.some(m => m.triangleCount > 0)).toBe(true);
	});

	test('boss extrude on existing body does not crash WASM engine', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		// Step 1: Create base body
		await createSketchWithRectangle(page);
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		// Step 2: Create a second sketch on the top face (z=10 plane)
		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 10], [0, 0, 1]));
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await page.waitForTimeout(200);

		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 1, x: -10, y: -10 });
			w.addSketchEntity({ type: 'Point', id: 2, x: 10, y: -10 });
			w.addSketchEntity({ type: 'Point', id: 3, x: 10, y: 10 });
			w.addSketchEntity({ type: 'Point', id: 4, x: -10, y: 10 });
			w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
			w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
			w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
			w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
		});
		await page.waitForTimeout(200);

		await page.evaluate(() => window.__waffle.finishSketch());
		await waitForFeatureCount(page, 3, 10000);
		await page.waitForTimeout(200);

		// Step 3: Extrude the boss sketch (triggers auto-union boolean)
		await applyExtrude(page, 5);
		await page.waitForTimeout(5000);

		// Step 4: ZERO crashes
		expectNoAnyCrash(crashTracker);

		// Step 5: Verify engine is fully functional
		const featureCount = await getFeatureCount(page);
		expect(featureCount).toBeGreaterThanOrEqual(2);

		const meshes = await getMeshes(page);
		expect(meshes.some(m => m.triangleCount > 0)).toBe(true);
	});
});
