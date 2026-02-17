/**
 * Property Editor panel tests — verifies property display, editing, and snap settings.
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
	waitForEntityCount,
	waitForFeatureCount,
	hasMeshWithGeometry,
} from './helpers/state.js';

const FEATURE_ITEM = '.tree-item:not(.origin-item)';

async function createSketchAndExtrude(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('pe-sketch-draw-failed');
	}
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('pe-sketch-finish-failed');
	}
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
		await waffle.dumpState('pe-extrude-failed');
	}
}

test.describe('property editor', () => {
	test('selecting extrude shows depth, symmetric, and cut fields', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Click the extrude tree item (second feature item)
		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await extrudeItem.click();
		await waffle.page.waitForTimeout(300);

		// Verify feature type shows "Extrude"
		const featureType = waffle.page.locator('.property-editor .feature-type');
		await expect(featureType).toBeVisible();
		await expect(featureType).toHaveText('Extrude');

		// Verify 3 field rows (Depth, Symmetric, Cut)
		const fieldRows = waffle.page.locator('.property-editor .fields .field-row');
		await expect(fieldRows).toHaveCount(3);
	});

	test('selecting sketch shows entity and constraint info', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Click the sketch tree item (first feature item)
		const sketchItem = waffle.page.locator(FEATURE_ITEM).nth(0);
		await sketchItem.click();
		await waffle.page.waitForTimeout(300);

		// Verify feature type shows "Sketch"
		const featureType = waffle.page.locator('.property-editor .feature-type');
		await expect(featureType).toBeVisible();
		await expect(featureType).toHaveText('Sketch');

		// Verify field-info elements are visible (Entities and Constraints)
		const infoFields = waffle.page.locator('.property-editor .field-info');
		const count = await infoFields.count();
		expect(count).toBeGreaterThanOrEqual(1);
	});

	test('no selection shows empty state message', async ({ waffle }) => {
		// Fresh state — no features created, no selection
		const emptyState = waffle.page.locator('.property-editor .empty-state');
		await expect(emptyState).toBeVisible();
		await expect(emptyState).toHaveText('Select a feature to edit its properties');
	});

	test('editing depth value triggers update', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Click extrude tree item
		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await extrudeItem.click();
		await waffle.page.waitForTimeout(300);

		// Find the depth number input (first .field-input[type="number"])
		const depthInput = waffle.page.locator('.property-editor .field-input[type="number"]').first();
		await expect(depthInput).toBeVisible();

		// Clear and fill with new value
		await depthInput.fill('20');
		// Trigger onchange by pressing Tab
		await waffle.page.keyboard.press('Tab');

		// Wait for 300ms debounce + processing
		await waffle.page.waitForTimeout(500);

		// Verify the input still shows '20'
		await expect(depthInput).toHaveValue('20');

		// Verify mesh still exists (rebuild didn't crash)
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('toggling cut checkbox updates feature', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Click extrude tree item
		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await extrudeItem.click();
		await waffle.page.waitForTimeout(300);

		// Find checkboxes — Cut is the second one (after Symmetric)
		const checkboxes = waffle.page.locator('.property-editor .field-checkbox');
		const cutCheckbox = checkboxes.nth(1);
		await expect(cutCheckbox).toBeVisible();

		// Click to check it
		await cutCheckbox.check();

		// Wait for debounce
		await waffle.page.waitForTimeout(500);

		// Verify it's now checked
		await expect(cutCheckbox).toBeChecked();
	});

	test('property editor shows snap settings in sketch mode', async ({ waffle }) => {
		// Enter sketch mode
		await clickSketch(waffle.page);

		// Verify snap settings section header
		const sectionHeader = waffle.page.locator('.property-editor .section-header');
		await expect(sectionHeader).toBeVisible();
		await expect(sectionHeader).toHaveText('Snap Settings');

		// Verify 3 field inputs for snap settings
		const snapInputs = waffle.page.locator('.property-editor .field-input');
		await expect(snapInputs).toHaveCount(3);
	});
});
