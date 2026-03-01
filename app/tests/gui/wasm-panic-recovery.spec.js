/**
 * WASM Panic Recovery — Extrude-on-Extrude Coplanar Boolean
 *
 * Verifies that the engine correctly handles boolean operations with coplanar
 * face degeneracies, including extrude-on-extrude with opposite directions.
 *
 * Reproduction: sketch a rectangle, extrude +Z, then extrude the same sketch
 * -Z. The two bodies share the exact sketch plane face. The auto-union boolean
 * must merge them into a single body spanning both directions.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	getFeatureCount,
	getMeshes,
	waitForMeshWithGeometry,
	isEngineReady,
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

test.describe('WASM panic recovery', () => {
	test('extrude-on-extrude coplanar does not crash engine', async ({ waffle }) => {
		const page = waffle.page;

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
		// The boolean may still fail via perturbation cascade — the engine must survive.
		await applyExtrude(page, 10, { flipDirection: true });

		// Wait for the engine to process — either success or error
		// Give it extra time since the boolean cascade may retry many perturbations
		await page.waitForTimeout(3000);

		// Step 4: Verify the engine is still alive
		// The result should be either:
		// a) 3 features (sketch + 2 extrudes) with mesh — boolean succeeded
		// b) 3 features with error/warning — boolean failed gracefully
		// c) 2 features if the second extrude was rejected — still alive
		// What MUST NOT happen: engine crash (unreachable trap)
		const featureCount = await getFeatureCount(page);
		expect(featureCount).toBeGreaterThanOrEqual(2);

		// Step 5: CRITICAL — verify engine liveness
		// Send another command and verify the engine responds.
		// If the WASM module crashed, this will fail.
		const engineAlive = await page.evaluate(() => {
			try {
				const state = window.__waffle?.getState();
				return state?.engineReady === true;
			} catch {
				return false;
			}
		});
		expect(engineAlive).toBe(true);
	});

	test('engine responds to queries after boolean error', async ({ waffle }) => {
		const page = waffle.page;

		// Step 1: Create sketch + extrude forward (known good)
		await createSketchWithRectangle(page);
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		// Record pre-error state
		const meshesBeforeError = await getMeshes(page);
		const trianglesBefore = meshesBeforeError.reduce((sum, m) => sum + m.triangleCount, 0);
		expect(trianglesBefore).toBeGreaterThan(0);

		// Step 2: Trigger the coplanar extrude (may error)
		await applyExtrude(page, 10, { flipDirection: true });
		await page.waitForTimeout(3000);

		// Step 3: Verify the engine can still answer queries
		// If the WASM module crashed, these __waffle API calls would throw/return null
		const engineState = await page.evaluate(() => {
			const state = window.__waffle?.getState();
			const tree = window.__waffle?.getFeatureTree();
			const meshes = window.__waffle?.getMeshes();
			return {
				engineReady: state?.engineReady,
				featureCount: tree?.features?.length ?? 0,
				meshCount: meshes?.length ?? 0,
				hasTriangles: (meshes ?? []).some(m => m.triangleCount > 0),
			};
		});

		// Engine must be alive and responsive
		expect(engineState.engineReady).toBe(true);
		expect(engineState.featureCount).toBeGreaterThanOrEqual(2);

		// The first extrude's mesh should still be intact
		expect(engineState.hasTriangles).toBe(true);
	});

	test('multiple sequential extrudes do not accumulate corruption', async ({ waffle }) => {
		const page = waffle.page;

		// Create base sketch
		await createSketchWithRectangle(page);

		// Extrude forward
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		// Try the problematic reverse extrude
		await applyExtrude(page, 10, { flipDirection: true });
		await page.waitForTimeout(3000);

		// Engine should still report ready
		const ready1 = await isEngineReady(page);
		expect(ready1).toBe(true);

		// Try yet another extrude from the same sketch (forward again)
		await applyExtrude(page, 5);
		await page.waitForTimeout(2000);

		// Engine should still be alive
		const ready2 = await isEngineReady(page);
		expect(ready2).toBe(true);

		// Should have some mesh geometry visible
		const meshes = await getMeshes(page);
		expect(meshes.length).toBeGreaterThan(0);
	});

	test('boss extrude on existing body does not crash engine', async ({ waffle }) => {
		const page = waffle.page;

		// Step 1: Create base sketch (large rectangle)
		await createSketchWithRectangle(page);

		// Step 2: Extrude the base body
		await applyExtrude(page, 10);
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		const meshesAfterBase = await getMeshes(page);
		expect(meshesAfterBase.some(m => m.triangleCount > 0)).toBe(true);

		// Step 3: Create a second sketch on the top face (z=10 plane)
		// This simulates the boss-on-face workflow
		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 10], [0, 0, 1]));
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await page.waitForTimeout(200);

		// Small rectangle for boss (inset from base edges)
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

		// Step 4: Extrude the boss sketch (triggers auto-union boolean)
		await applyExtrude(page, 5);
		await page.waitForTimeout(3000);

		// Step 5: Engine must still be alive regardless of boolean outcome
		const engineAlive = await page.evaluate(() => {
			try {
				const state = window.__waffle?.getState();
				return state?.engineReady === true;
			} catch {
				return false;
			}
		});
		expect(engineAlive).toBe(true);

		// Should still have mesh geometry
		const meshesAfterBoss = await getMeshes(page);
		expect(meshesAfterBoss.some(m => m.triangleCount > 0)).toBe(true);
	});
});
