/**
 * Circle-cut workflow tests — verify that drawing a circle on an existing
 * body and extruding as a cut produces a valid result without WASM crashes.
 *
 * Reproduces the reported GUI crash:
 *   "shell assembly failed: v2: 12 open edges after all levels"
 *
 * The GUI always creates true NURBS circles (via CircleProfile/rsweep),
 * not polygon approximations.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickCircle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle, drawCircle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';
import { getFirstFaceRef } from './helpers/geometry.js';

/**
 * Helper: select a face ref programmatically.
 */
async function selectFaceRef(page, ref) {
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(200);
}

/**
 * Helper: create a base box via sketch rectangle + extrude.
 */
async function createBaseBox(waffle, { depth = '10' } = {}) {
	await clickSketch(waffle.page, 'front');
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('ccw-base-sketch-failed');
	}
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('ccw-base-finish-failed');
	}
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill(depth);
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
		await waffle.dumpState('ccw-base-extrude-failed');
	}
}

test.describe('circle cut workflow', () => {
	test('box + circle cut: no WASM crash', async ({ waffle }) => {
		const crashTracker = collectCrashErrors(waffle.page);

		// Step 1: Create base box
		await createBaseBox(waffle);
		await waitForMeshWithGeometry(waffle.page);

		// Step 2: Select top face and start sketch
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		// Step 3: Draw circle on the face
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 40, 0);
		try { await waitForEntityCount(waffle.page, 2, 3000); } catch {
			await waffle.dumpState('ccw-circle-draw-failed');
		}

		// Step 4: Finish sketch
		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 3, 10000); } catch {
			await waffle.dumpState('ccw-circle-finish-failed');
		}

		// Step 5: Extrude with cut enabled
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('5');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 4, 15000); } catch {
			await waffle.dumpState('ccw-circle-cut-extrude-failed');
		}

		// Step 6: Verify mesh is present and no crashes
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
		expectNoAnyCrash(crashTracker);
	});
});
