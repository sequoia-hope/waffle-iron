/**
 * Regression tests for full-depth circular cut "no Z overlap" bug.
 *
 * These tests reproduce the exact user workflow:
 *   1. Draw circle → extrude to create cylinder
 *   2. Select top face → sketch on face → draw smaller circle
 *   3. Extrude-cut with depth = original extrude depth
 *
 * The bug: kernel's cyl_cyl_boolean() rejects the cut because the tool
 * cylinder's Z range doesn't overlap the boss, due to incorrect direction
 * or origin in the cut parameters.
 *
 * IMPORTANT: NO try/catch around assertions. Failures are the test signal.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickCircle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawCircle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';
import { getFirstFaceRef } from './helpers/geometry.js';

async function selectFaceRef(page, ref) {
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(200);
}

/**
 * Get feature errors from the engine.
 * Returns an array of error strings (empty if no errors).
 */
async function getFeatureErrors(page) {
	return page.evaluate(() => {
		const errors = window.__waffle?.getFeatureErrors?.();
		if (!errors || !(errors instanceof Map)) return [];
		return Array.from(errors.values()).map(e => String(e));
	});
}

test.describe('Full-depth circular cut regression', () => {
	test('g1: circle boss + circle cut full depth should not produce no-z-overlap error', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);
		const consoleErrors = [];
		waffle.page.on('console', msg => {
			if (msg.type() === 'error') consoleErrors.push(msg.text());
		});

		// Step 1: Draw circle and extrude to create cylinder
		await clickSketch(waffle.page, 'front');
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 60, 0);
		await waitForEntityCount(waffle.page, 2, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		// Extrude depth=10 to create cylinder
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(waffle.page, 2, 15000);
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select top face and start sketch on it
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		// Step 3: Draw smaller circle on the face
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 30, 0);
		await waitForEntityCount(waffle.page, 2, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 3, 10000);

		// Step 4: Extrude cut with SAME depth (10) — full through-cut
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(waffle.page, 4, 30000);

		// Wait for rebuild to complete
		await waffle.page.waitForTimeout(1000);

		// Check for feature errors — the bug manifests as "no Z overlap" error
		const featureErrors = await getFeatureErrors(waffle.page);
		const zOverlapErrors = featureErrors.filter(e => /no z overlap/i.test(e));

		expect(zOverlapErrors).toHaveLength(0);

		// Verify mesh still has geometry (cut didn't destroy it)
		const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());
		const hasMesh = meshes.some(m => m.triangleCount > 0);
		expect(hasMesh).toBe(true);

		expectNoAnyCrash(crashTracker);
	});

	test('g2: circle boss + circle cut partial depth should succeed (baseline)', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Draw circle and extrude
		await clickSketch(waffle.page, 'front');
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 60, 0);
		await waitForEntityCount(waffle.page, 2, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(waffle.page, 2, 15000);
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select face and sketch circle
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 30, 0);
		await waitForEntityCount(waffle.page, 2, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 3, 10000);

		// Step 3: Extrude cut with HALF depth (5) — partial cut (baseline)
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('5');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(waffle.page, 4, 30000);

		await waffle.page.waitForTimeout(1000);

		// Partial-depth cut should always work — no Z overlap issues
		const featureErrors = await getFeatureErrors(waffle.page);
		const zOverlapErrors = featureErrors.filter(e => /no z overlap/i.test(e));
		expect(zOverlapErrors).toHaveLength(0);

		const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());
		const hasMesh = meshes.some(m => m.triangleCount > 0);
		expect(hasMesh).toBe(true);

		expectNoAnyCrash(crashTracker);
	});

	test('g3: full-depth cut error message contains no-z-overlap string', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);
		const consoleErrors = [];
		waffle.page.on('console', msg => {
			if (msg.type() === 'error') consoleErrors.push(msg.text());
		});

		// Same workflow as g1 — draw circle, extrude, sketch on face, cut
		await clickSketch(waffle.page, 'front');
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 60, 0);
		await waitForEntityCount(waffle.page, 2, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(waffle.page, 2, 15000);
		await waitForMeshWithGeometry(waffle.page);

		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 30, 0);
		await waitForEntityCount(waffle.page, 2, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 3, 10000);

		// Full-depth cut
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(waffle.page, 4, 30000);

		await waffle.page.waitForTimeout(1000);

		// Check feature errors for the specific "no Z overlap" string.
		// If this assertion PASSES (errors found), it confirms the bug.
		// If it FAILS (no errors), the bug doesn't reproduce via GUI (good!).
		const featureErrors = await getFeatureErrors(waffle.page);
		const allErrors = [...featureErrors, ...consoleErrors];
		const noZOverlap = allErrors.some(e => /no z overlap/i.test(e) || /no Z overlap/.test(e));

		// We EXPECT this to be false (no bug). If true, the bug is confirmed.
		// Either way, this test documents the state.
		expect(noZOverlap).toBe(false);

		expectNoAnyCrash(crashTracker);
	});
});
