/**
 * Sketch-on-face full E2E workflow tests.
 *
 * Builds on the helpers from sketch-on-face.spec.js to test complete
 * extrude -> sketch-on-face -> draw -> extrude-again workflows.
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
	collectCrashErrors,
	expectNoAnyCrash,
	getFeatureCount,
	getEntityCount,
	waitForEntityCount,
	waitForFeatureCount,
	hasMeshWithGeometry,
} from './helpers/state.js';

/**
 * Helper: create a sketch + extruded box via real GUI events.
 */
async function createExtrudedBox(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('sof-wf-box-draw-failed');
	}

	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('sof-wf-box-finish-failed');
	}

	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
		await waffle.dumpState('sof-wf-extrude-failed');
	}
}

/**
 * Helper: get a face GeomRef from the first mesh that has face ranges.
 */
async function getFirstFaceRef(page) {
	return page.evaluate(() => {
		const meshes = window.__waffle.getMeshes();
		const mesh = meshes.find(m => m.faceRangeCount > 0);
		if (!mesh || mesh.faceRanges.length === 0) return null;
		return mesh.faceRanges[0].geom_ref;
	});
}

/**
 * Helper: select a face ref programmatically.
 */
async function selectFaceRef(page, ref) {
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(200);
}

test.describe('sketch-on-face full workflow', () => {
	test('extrude box -> sketch on face -> draw circle -> finish = 3 features', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		await createExtrudedBox(waffle);

		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();

		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		// Draw a circle on the face
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 40, 0);
		await waitForEntityCount(waffle.page, 2, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 3, 10000);

		const featureCount = await getFeatureCount(waffle.page);
		expect(featureCount).toBe(3);

		expectNoAnyCrash(crashes);
	});

	test('sketch-on-face can be extruded as cut', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		await createExtrudedBox(waffle);

		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();

		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		// Draw rectangle on face
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -30, -20, 30, 20);
		await waitForEntityCount(waffle.page, 8, 5000);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 3, 10000);

		// Extrude the face sketch as a cut
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('5');
		await waffle.page.locator('[data-testid="extrude-cut"]').check();
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		await waitForFeatureCount(waffle.page, 4, 15000);
		const featureCount = await getFeatureCount(waffle.page);
		expect(featureCount).toBe(4);

		// Mesh should still exist
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		expectNoAnyCrash(crashes);
	});

	test('sketch-on-face starts with empty entity count', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		await createExtrudedBox(waffle);

		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();

		await selectFaceRef(waffle.page, faceRef);
		await clickSketch(waffle.page);

		// New sketch should start with 0 entities
		const entityCount = await getEntityCount(waffle.page);
		expect(entityCount).toBe(0);

		expectNoAnyCrash(crashes);
	});
});
