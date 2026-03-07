/**
 * WASM Panic Recovery — Extrude-on-Extrude Coplanar Boolean
 *
 * Verifies that the engine correctly handles boolean operations with coplanar
 * face degeneracies, including extrude-on-extrude with opposite directions.
 *
 * With panic=unwind enabled (via nightly + -Zbuild-std), catch_unwind in the
 * boolean cascade catches truck panics and returns a graceful error instead
 * of killing the module. These tests verify ZERO crashes.
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
		w.addSketchEntity({ type: 'Point', id: 1, x: -30, y: -30, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: 30, y: -30, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: 30, y: 30, construction: false });
		w.addSketchEntity({ type: 'Point', id: 4, x: -30, y: 30, construction: false });
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

test.describe('WASM panic recovery', () => {
	test('extrude-on-extrude coplanar recovers from crash', async ({ waffle }) => {
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

		// Step 5: Verify the engine is still alive and can answer queries
		const canQuery = await page.evaluate(() => {
			try {
				const tree = window.__waffle?.getFeatureTree();
				return tree !== null && tree !== undefined;
			} catch {
				return false;
			}
		});
		expect(canQuery).toBe(true);
	});

	test('engine responds to queries after boolean crash + recovery', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		// Step 1: Create sketch + extrude forward (known good)
		await createSketchWithRectangle(page);
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		// Record pre-error state
		const meshesBeforeError = await getMeshes(page);
		const trianglesBefore = meshesBeforeError.reduce((sum, m) => sum + m.triangleCount, 0);
		expect(trianglesBefore).toBeGreaterThan(0);

		// Step 2: Trigger the coplanar extrude
		// catch_unwind catches the boolean panic gracefully
		await applyExtrude(page, 10, { flipDirection: true });
		await page.waitForTimeout(5000);

		// Step 3: ZERO crashes
		expectNoAnyCrash(crashTracker);

		// Step 4: Verify the engine can still answer queries
		const queryWorks = await page.evaluate(() => {
			try {
				const tree = window.__waffle?.getFeatureTree();
				return typeof tree === 'object' && tree !== null;
			} catch {
				return false;
			}
		});
		expect(queryWorks).toBe(true);
	});

	test('multiple sequential extrudes with crash recovery', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		// Create base sketch
		await createSketchWithRectangle(page);

		// Extrude forward
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		// Reverse extrude — catch_unwind handles the boolean panic
		await applyExtrude(page, 10, { flipDirection: true });
		await page.waitForTimeout(5000);

		// ZERO crashes
		expectNoAnyCrash(crashTracker);

		// Engine should still be queryable
		const alive = await page.evaluate(() => {
			try {
				return window.__waffle?.getFeatureTree() !== null;
			} catch {
				return false;
			}
		});
		expect(alive).toBe(true);
	});

	test('boss extrude on existing body recovers from crash', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		// Step 1: Create base sketch (large rectangle)
		await createSketchWithRectangle(page);

		// Step 2: Extrude the base body
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		const meshesAfterBase = await getMeshes(page);
		expect(meshesAfterBase.some(m => m.triangleCount > 0)).toBe(true);

		// Step 3: Create a second sketch on the top face (z=10 plane)
		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 10], [0, 0, 1]));
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await page.waitForTimeout(200);

		// Small rectangle for boss (inset from base edges)
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 1, x: -10, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 2, x: 10, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 3, x: 10, y: 10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 4, x: -10, y: 10, construction: false });
			w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
			w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
			w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
			w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
		});
		await page.waitForTimeout(200);

		await page.evaluate(() => window.__waffle.finishSketch());
		await waitForFeatureCount(page, 3, 10000);
		await page.waitForTimeout(200);

		// Step 4: Extrude the boss sketch (triggers auto-union boolean)
		await applyExtrude(page, 5);
		await page.waitForTimeout(5000);

		// Step 5: ZERO crashes
		expectNoAnyCrash(crashTracker);

		// Engine should still be queryable
		const canQuery = await page.evaluate(() => {
			try {
				return window.__waffle?.getFeatureTree() !== null;
			} catch {
				return false;
			}
		});
		expect(canQuery).toBe(true);
	});
});
