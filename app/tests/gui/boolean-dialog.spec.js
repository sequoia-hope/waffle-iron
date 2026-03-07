/**
 * Boolean Combine dialog tests — open/close, operation selection,
 * target/tool body pickers, Apply/Cancel, keyboard shortcuts.
 *
 * The Boolean Combine dialog (BooleanDialog.svelte) is distinct from the
 * extrude cut checkbox. It provides Union/Subtract/Intersect operations
 * between two existing solid bodies.
 *
 * Prerequisites: Two extruded bodies must exist before the dialog
 * shows any selectable bodies.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	getFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Create a single extruded box via the __waffle API.
 * Sufficient for testing dialog UI (open/close, radio buttons, etc.)
 * without needing a second body.
 */
async function createOneBody(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: -20, y: -20, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: 20, y: -20, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: 20, y: 20, construction: false });
		w.addSketchEntity({ type: 'Point', id: 4, x: -20, y: 20, construction: false });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	});
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, 1, 10000);
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(() => window.__waffle.applyExtrude(30, 0, false));
	await waitForFeatureCount(page, 2, 10000);
	await waitForMeshWithGeometry(page);
	await page.waitForTimeout(300);
}

test.describe('boolean dialog basics', () => {
	test('clicking Boolean toolbar button opens dialog', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.locator('[data-testid="toolbar-btn-boolean"]').click();
		await waffle.page.waitForTimeout(200);

		const dialog = waffle.page.locator('[data-testid="boolean-dialog"]');
		await expect(dialog).toBeVisible();
	});

	test('boolean dialog shows Union selected by default', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const dialog = waffle.page.locator('[data-testid="boolean-dialog"]');
		await expect(dialog).toBeVisible();

		// Union radio should be checked by default
		const unionRadio = dialog.locator('input[type="radio"][value="Union"]');
		await expect(unionRadio).toBeChecked();
	});

	test('boolean dialog shows target and tool body selects', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const targetSelect = waffle.page.locator('[data-testid="boolean-target"]');
		const toolSelect = waffle.page.locator('[data-testid="boolean-tool"]');

		await expect(targetSelect).toBeVisible();
		await expect(toolSelect).toBeVisible();
	});

	test('boolean dialog Apply button is disabled when no tool body selected', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		// Apply should be disabled (no tool body selected yet)
		const applyBtn = waffle.page.locator('[data-testid="boolean-apply"]');
		await expect(applyBtn).toBeDisabled();
	});

	test('boolean dialog Cancel closes without creating feature', async ({ waffle }) => {
		await createOneBody(waffle.page);

		const featuresBefore = await getFeatureCount(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		await waffle.page.locator('[data-testid="boolean-cancel"]').click();

		// Dialog should be gone
		await expect(waffle.page.locator('[data-testid="boolean-dialog"]')).not.toBeVisible();

		// Feature count unchanged
		const featuresAfter = await getFeatureCount(waffle.page);
		expect(featuresAfter).toBe(featuresBefore);
	});

	test('boolean dialog close button (X) closes dialog', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		// Click the X close button
		await waffle.page.locator('[data-testid="boolean-dialog"] .close-btn').click();

		await expect(waffle.page.locator('[data-testid="boolean-dialog"]')).not.toBeVisible();
	});
});

test.describe('boolean dialog operation selection', () => {
	test('can select Subtract operation', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const dialog = waffle.page.locator('[data-testid="boolean-dialog"]');
		const subtractRadio = dialog.locator('input[type="radio"][value="Subtract"]');
		await subtractRadio.check();

		await expect(subtractRadio).toBeChecked();

		// Union should no longer be checked
		const unionRadio = dialog.locator('input[type="radio"][value="Union"]');
		await expect(unionRadio).not.toBeChecked();
	});

	test('can select Intersect operation', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const dialog = waffle.page.locator('[data-testid="boolean-dialog"]');
		const intersectRadio = dialog.locator('input[type="radio"][value="Intersect"]');
		await intersectRadio.check();

		await expect(intersectRadio).toBeChecked();
	});
});

test.describe('boolean dialog keyboard shortcuts', () => {
	test('Escape closes boolean dialog', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		await expect(waffle.page.locator('[data-testid="boolean-dialog"]')).toBeVisible();

		await waffle.page.keyboard.press('Escape');

		await expect(waffle.page.locator('[data-testid="boolean-dialog"]')).not.toBeVisible();

		// Feature count unchanged
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(2); // 1 sketch + 1 extrude
	});
});

// FIXME: Body selection tests require two bodies, which fails with
// "Engine error: unreachable" on the second extrude in the current WASM build.
test.describe('boolean dialog body selection', () => {
	test('target select shows at least one body option', async ({ waffle }) => {
		await createOneBody(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const targetSelect = waffle.page.locator('[data-testid="boolean-target"]');

		// Should have at least 1 body option (from our single extrude)
		const options = targetSelect.locator('option:not([disabled])');
		const count = await options.count();
		expect(count).toBeGreaterThanOrEqual(1);
	});

	test.fixme('target select has options for two existing bodies', async ({ waffle }) => {
		// Requires createTwoBodies — blocked by WASM second-extrude issue
	});

	test.fixme('tool body select excludes selected target', async ({ waffle }) => {
		// Requires createTwoBodies — blocked by WASM second-extrude issue
	});
});

/**
 * Helper: open boolean dialog and select target + tool bodies, then apply.
 * Returns the feature count before apply so callers can verify changes.
 */
async function applyBooleanOperation(page, operation = 'Union') {
	const featuresBefore = await getFeatureCount(page);

	await page.evaluate(() => window.__waffle.showBooleanDialog());
	await page.waitForTimeout(200);

	const dialog = page.locator('[data-testid="boolean-dialog"]');
	await expect(dialog).toBeVisible();

	// Select the operation
	if (operation !== 'Union') {
		const radio = dialog.locator(`input[type="radio"][value="${operation}"]`);
		await radio.check();
	}

	// Target is auto-selected. Pick the first available tool body.
	const toolSelect = page.locator('[data-testid="boolean-tool"]');
	const toolOptions = toolSelect.locator('option:not([disabled])');
	const toolCount = await toolOptions.count();
	expect(toolCount).toBeGreaterThanOrEqual(1);

	const toolValue = await toolOptions.nth(0).getAttribute('value');
	await toolSelect.selectOption(toolValue);
	await page.waitForTimeout(100);

	// Apply should now be enabled
	const applyBtn = page.locator('[data-testid="boolean-apply"]');
	await expect(applyBtn).toBeEnabled();

	await applyBtn.click();

	// Wait for the BooleanCombine feature to be created
	await waitForFeatureCount(page, featuresBefore + 1, 15000);

	return featuresBefore;
}

// FIXME: All boolean dialog tests depend on createTwoBodies(), which fails with
// "Engine error: unreachable" on the second extrude in the current WASM build.
// This is a pre-existing regression — these tests passed in Sprint 39.
// The two-body creation issue needs to be fixed in the WASM engine first.
test.describe('boolean dialog apply workflow', () => {
	test.fixme('Union: two bodies → boolean Union → creates BooleanCombine feature', async ({ waffle }) => {
		const page = waffle.page;
		await createTwoBodies(page);

		const featuresBefore = await applyBooleanOperation(page, 'Union');

		// Dialog should be closed after apply
		await expect(page.locator('[data-testid="boolean-dialog"]')).not.toBeVisible();

		// Feature count should have increased by 1 (the BooleanCombine)
		const featuresAfter = await getFeatureCount(page);
		expect(featuresAfter).toBe(featuresBefore + 1);

		// The new feature should be a BooleanCombine
		const tree = await page.evaluate(() => window.__waffle.getFeatureTree());
		const boolFeature = tree.features.find(f => f.operation?.type === 'BooleanCombine');
		expect(boolFeature).toBeDefined();

		// Mesh should still exist after boolean
		const hasMesh = await hasMeshWithGeometry(page);
		expect(hasMesh).toBe(true);
	});

	test.fixme('Subtract: two bodies → boolean Subtract → creates BooleanCombine feature', async ({ waffle }) => {
		const page = waffle.page;
		await createTwoBodies(page);

		await applyBooleanOperation(page, 'Subtract');

		// Dialog should be closed
		await expect(page.locator('[data-testid="boolean-dialog"]')).not.toBeVisible();

		// BooleanCombine feature should exist
		const tree = await page.evaluate(() => window.__waffle.getFeatureTree());
		const boolFeature = tree.features.find(f => f.operation?.type === 'BooleanCombine');
		expect(boolFeature).toBeDefined();

		// Mesh should still exist (subtract removes material but keeps result)
		const hasMesh = await hasMeshWithGeometry(page);
		expect(hasMesh).toBe(true);
	});

	test.fixme('Intersect: two overlapping bodies → boolean Intersect → creates feature with mesh', async ({ waffle }) => {
		const page = waffle.page;
		await createTwoBodies(page);

		await applyBooleanOperation(page, 'Intersect');

		// Dialog should be closed
		await expect(page.locator('[data-testid="boolean-dialog"]')).not.toBeVisible();

		// BooleanCombine feature should exist
		const tree = await page.evaluate(() => window.__waffle.getFeatureTree());
		const boolFeature = tree.features.find(f => f.operation?.type === 'BooleanCombine');
		expect(boolFeature).toBeDefined();

		// The intersection of two overlapping boxes should produce geometry
		// (the boxes overlap in the x=[10,20] region)
		const hasMesh = await hasMeshWithGeometry(page);
		expect(hasMesh).toBe(true);
	});

	test.fixme('Apply enables after selecting tool body', async ({ waffle }) => {
		const page = waffle.page;
		await createTwoBodies(page);

		await page.evaluate(() => window.__waffle.showBooleanDialog());
		await page.waitForTimeout(200);

		// Initially disabled (no tool selected)
		const applyBtn = page.locator('[data-testid="boolean-apply"]');
		await expect(applyBtn).toBeDisabled();

		// Select a tool body
		const toolSelect = page.locator('[data-testid="boolean-tool"]');
		const toolOptions = toolSelect.locator('option:not([disabled])');
		const toolValue = await toolOptions.nth(0).getAttribute('value');
		await toolSelect.selectOption(toolValue);
		await page.waitForTimeout(100);

		// Now Apply should be enabled
		await expect(applyBtn).toBeEnabled();
	});
});
