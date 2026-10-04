/**
 * Advanced property editor tests — feature type display, depth editing,
 * cut toggle, snap settings visibility and editing, and feature switching.
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
	isSketchActive,
	waitForEntityCount,
	waitForFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
	getFeatureTree,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle and extrude it.
 */
async function createSketchAndExtrude(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);
	await waitForMeshWithGeometry(waffle.page);
}

test.describe('property editor presence', () => {
	test('property editor root has testid', async ({ waffle }) => {
		const propertyEditor = waffle.page.locator('[data-testid="property-editor"]');
		await expect(propertyEditor).toBeVisible();
	});
});

test.describe('property editor feature display', () => {
	test('selecting extrude shows type and name', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Click the extrude feature in the tree (second feature item, index 1)
		const extrudeItem = waffle.page.locator('[data-testid="feature-item-1"]');
		await extrudeItem.click();
		await waffle.page.waitForTimeout(300);

		// Verify the feature type shows "Extrude"
		const featureType = waffle.page.locator('[data-testid="prop-feature-type"]');
		await expect(featureType).toBeVisible();
		await expect(featureType).toHaveText('Extrude');
	});

	test('editing depth reaches the FEATURE, not just the input', async ({ waffle }) => {
		// This asserted only that a mesh still existed, which is true of a panel
		// edit that does nothing at all — and that is what it was: `handleChange`
		// cloned the `$state` feature tree's Proxy, `structuredClone` refused it
		// (DataCloneError, swallowed as an unhandled rejection in a timeout), and
		// `editFeature` was never called. The input kept the typed value and the
		// model kept the old one. The oracle has to be the TREE.
		await createSketchAndExtrude(waffle);

		const extrudeItem = waffle.page.locator('[data-testid="feature-item-1"]');
		await extrudeItem.click();
		await waffle.page.waitForTimeout(300);

		const depthInput = waffle.page.locator('[data-testid="prop-input-params.depth"]');
		await expect(depthInput).toBeVisible();
		// The panel edits the stored value directly (meters), so read the old
		// one rather than assuming it.
		const before = (await getFeatureTree(waffle.page)).features.find(
			(f) => f.operation?.type === 'Extrude'
		).operation.params.depth;
		expect(before).toBeGreaterThan(0);

		await depthInput.fill('0.025');
		await waffle.page.keyboard.press('Tab');

		// The debounce is 300 ms, then a rebuild.
		await waffle.page.waitForFunction(
			() => {
				const e = window.__waffle
					.getFeatureTree()
					.features.find((f) => f.operation?.type === 'Extrude');
				return e && Math.abs(e.operation.params.depth - 0.025) < 1e-12;
			},
			null,
			{ timeout: 10000 }
		);

		// And the rebuild really happened.
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('toggling cut updates params', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Select extrude feature
		const extrudeItem = waffle.page.locator('[data-testid="feature-item-1"]');
		await extrudeItem.click();
		await waffle.page.waitForTimeout(300);

		// Find the cut checkbox
		const cutCheckbox = waffle.page.locator('[data-testid="prop-input-params.cut"]');
		await expect(cutCheckbox).toBeVisible();

		// Toggle it on
		await cutCheckbox.check();
		await waffle.page.waitForTimeout(500);

		// Verify it's checked
		await expect(cutCheckbox).toBeChecked();
	});

	test('property editor updates on feature switch', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Click sketch feature (index 0)
		const sketchItem = waffle.page.locator('[data-testid="feature-item-0"]');
		await sketchItem.click();
		await waffle.page.waitForTimeout(300);

		const typeAfterSketch = waffle.page.locator('[data-testid="prop-feature-type"]');
		await expect(typeAfterSketch).toBeVisible();
		await expect(typeAfterSketch).toHaveText('Sketch');

		// Click extrude feature (index 1)
		const extrudeItem = waffle.page.locator('[data-testid="feature-item-1"]');
		await extrudeItem.click();
		await waffle.page.waitForTimeout(300);

		const typeAfterExtrude = waffle.page.locator('[data-testid="prop-feature-type"]');
		await expect(typeAfterExtrude).toHaveText('Extrude');
	});
});

test.describe('snap settings in property editor', () => {
	test('snap settings visible in sketch mode', async ({ waffle }) => {
		// Enter sketch mode
		await clickSketch(waffle.page);
		expect(await isSketchActive(waffle.page)).toBe(true);

		// Snap setting input should be visible
		const coincidentInput = waffle.page.locator('[data-testid="snap-coincidentPx"]');
		await expect(coincidentInput).toBeVisible();

		// Finish sketch — snap settings should disappear
		await clickFinishSketch(waffle.page);
		await waffle.page.waitForTimeout(300);

		await expect(coincidentInput).not.toBeVisible();
	});

	test('changing snap setting updates value', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const coincidentInput = waffle.page.locator('[data-testid="snap-coincidentPx"]');
		await expect(coincidentInput).toBeVisible();

		// Change the value to 12
		await coincidentInput.fill('12');
		await waffle.page.keyboard.press('Tab');
		await waffle.page.waitForTimeout(300);

		// Verify via __waffle API
		const snapSettings = await waffle.page.evaluate(() =>
			window.__waffle?.getSnapSettings?.()
		);
		expect(snapSettings).toBeDefined();
		expect(snapSettings.coincidentPx).toBe(12);
	});

	test('snap settings has 4 inputs', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// All 4 snap setting inputs should be present
		await expect(waffle.page.locator('[data-testid="snap-coincidentPx"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="snap-onEntityPx"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="snap-hvAngleDeg"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="snap-previewPx"]')).toBeVisible();
	});
});
