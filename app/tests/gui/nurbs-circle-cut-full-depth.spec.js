/**
 * NURBS circle full-depth cut — exact user-reported failure scenario.
 *
 * Steps:
 *   1. Draw a NURBS circle → extrude to create cylinder
 *   2. Select top face → draw smaller NURBS circle
 *   3. Extrude-cut with depth = original extrude depth (full through-cut)
 *
 * This creates exact coplanarity between the cut tool caps and the cylinder
 * caps. Previously failed with "v2: 12 open edges after all levels".
 * Fixed by B23: cut_eps removed, coplanar containment pipeline handles it.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickCircle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawCircle, drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';
import { getFirstFaceRef } from './helpers/geometry.js';

async function selectFaceRef(page, ref) {
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(200);
}

test.describe('NURBS circle full-depth cut', () => {
	test('cylinder + circle cut same depth: no crash', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Draw circle and extrude to create cylinder
		await clickSketch(waffle.page, 'front');
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 60, 0);
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('ncd-circle-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('ncd-finish-sketch-failed');
		}

		// Extrude depth=10 to create cylinder
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 2, 15000);
		} catch {
			await waffle.dumpState('ncd-extrude-failed');
		}
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select top face and start sketch on it
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		// Step 3: Draw smaller circle on the face
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 30, 0);
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('ncd-inner-circle-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 3, 10000);
		} catch {
			await waffle.dumpState('ncd-inner-finish-failed');
		}

		// Step 4: Extrude cut with SAME depth (10) — full through-cut
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 4, 30000);
		} catch {
			await waffle.dumpState('ncd-cut-extrude-failed');
		}

		// Verify: mesh still exists and no crashes
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
		expectNoAnyCrash(crashTracker);
	});

	test('box + circle cut same depth: no crash', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Create box via rectangle sketch + extrude
		await clickSketch(waffle.page, 'front');
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('ncd-box-sketch-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('ncd-box-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 2, 15000);
		} catch {
			await waffle.dumpState('ncd-box-extrude-failed');
		}
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select face, sketch circle on it
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 40, 0);
		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('ncd-box-circle-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 3, 10000);
		} catch {
			await waffle.dumpState('ncd-box-circle-finish-failed');
		}

		// Step 3: Extrude cut with same depth (10) — full through-cut
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try {
			await waitForFeatureCount(waffle.page, 4, 30000);
		} catch {
			await waffle.dumpState('ncd-box-cut-failed');
		}

		// Verify
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
		expectNoAnyCrash(crashTracker);
	});
});
