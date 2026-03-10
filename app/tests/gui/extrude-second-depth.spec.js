/**
 * Extrude second depth passthrough tests.
 *
 * Verifies that second direction depth values actually propagate to feature params
 * in the feature tree.
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
	getFeatureTree,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle and finish it.
 */
async function createSketchWithRect(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);
}

test.describe('extrude second depth', () => {
	test('Two Depths stores both values in feature params', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await createSketchWithRect(waffle);

		await clickExtrude(page);

		// Set primary depth
		await page.locator('[data-testid="extrude-depth"]').fill('15');

		// Set 2nd direction to "Two Depths" (value="Blind")
		await page.locator('[data-testid="extrude-second-dir"]').selectOption('Blind');
		await page.waitForTimeout(200);

		// Set second depth
		const secondDepthInput = page.locator('[data-testid="extrude-second-depth"]');
		await expect(secondDepthInput).toBeVisible();
		await secondDepthInput.fill('8');

		// Apply
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);

		// Verify feature tree contains the extrude with second direction
		const tree = await getFeatureTree(page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature).toBeTruthy();

		const params = extrudeFeature.operation.params;
		expect(params).toBeTruthy();

		// Second direction should be present with Blind type
		expect(params.second_direction).toBeTruthy();
		expect(params.second_direction.type).toBe('Blind');
		expect(params.second_direction.depth).toBeGreaterThan(0);

		expectNoAnyCrash(crashes);
	});

	test('Symmetric flag stored in feature params', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await createSketchWithRect(waffle);

		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('10');

		// Set 2nd direction to Symmetric
		await page.locator('[data-testid="extrude-second-dir"]').selectOption('Symmetric');
		await page.waitForTimeout(200);

		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);

		const tree = await getFeatureTree(page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature).toBeTruthy();

		const params = extrudeFeature.operation.params;
		expect(params.symmetric).toBe(true);

		expectNoAnyCrash(crashes);
	});

	test('second depth accepts decimal values', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await createSketchWithRect(waffle);

		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('10');

		// Set Two Depths with decimal second depth
		await page.locator('[data-testid="extrude-second-dir"]').selectOption('Blind');
		await page.waitForTimeout(200);

		await page.locator('[data-testid="extrude-second-depth"]').fill('7.5');

		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);

		// Feature should be created without error
		const tree = await getFeatureTree(page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature).toBeTruthy();

		const params = extrudeFeature.operation.params;
		expect(params.second_direction).toBeTruthy();
		expect(params.second_direction.type).toBe('Blind');
		expect(params.second_direction.depth).toBeGreaterThan(0);

		expectNoAnyCrash(crashes);
	});
});
