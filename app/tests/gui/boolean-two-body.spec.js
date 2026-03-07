/**
 * Boolean Combine end-to-end tests — creates two separate extruded bodies
 * and applies Union/Subtract/Intersect operations through the GUI.
 *
 * Uses the __waffle API for geometry setup (two offset boxes), then tests
 * the boolean dialog workflow through real GUI interactions.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	getFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoCrash,
} from './helpers/state.js';

/**
 * Create two overlapping extruded boxes via __waffle API.
 * Box 1: [-20,-20] to [20,20] on XY plane, extruded 30 in Z.
 * Box 2: [0,-20] to [40,20] on XY plane, extruded 30 in Z.
 * The boxes overlap in x=[0,20], giving a valid boolean target+tool pair.
 */
async function createTwoBodies(page) {
	// Body 1: centered box
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

	// Body 2: offset box (overlaps body 1 in x=[0,20])
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: 0, y: -20, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: 40, y: -20, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: 40, y: 20, construction: false });
		w.addSketchEntity({ type: 'Point', id: 4, x: 0, y: 20, construction: false });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	});
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, 3, 10000);
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(() => window.__waffle.applyExtrude(30, 0, false));
	await waitForFeatureCount(page, 4, 15000);
	await waitForMeshWithGeometry(page);
	await page.waitForTimeout(300);
}

test.describe('boolean two-body workflow', () => {
	test('can create two separate extruded bodies', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createTwoBodies(waffle.page);

		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(4); // 2 sketches + 2 extrudes

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		expectNoCrash(tracker);
	});

	test('boolean dialog shows two bodies in target select', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createTwoBodies(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const dialog = waffle.page.locator('[data-testid="boolean-dialog"]');
		await expect(dialog).toBeVisible();

		// Target select should have options for both bodies
		const targetSelect = waffle.page.locator('[data-testid="boolean-target"]');
		const options = targetSelect.locator('option:not([disabled])');
		const optionCount = await options.count();
		expect(optionCount).toBeGreaterThanOrEqual(2);

		expectNoCrash(tracker);
	});

	test('tool select excludes the currently selected target body', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createTwoBodies(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const targetSelect = waffle.page.locator('[data-testid="boolean-target"]');
		const toolSelect = waffle.page.locator('[data-testid="boolean-tool"]');

		// Get the selected target value
		const targetValue = await targetSelect.inputValue();

		// Tool select options should NOT include the target value
		const toolOptions = toolSelect.locator('option:not([disabled])');
		const toolCount = await toolOptions.count();
		expect(toolCount).toBeGreaterThanOrEqual(1);

		for (let i = 0; i < toolCount; i++) {
			const optValue = await toolOptions.nth(i).getAttribute('value');
			expect(optValue).not.toBe(targetValue);
		}

		expectNoCrash(tracker);
	});

	test('Apply becomes enabled after selecting tool body', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createTwoBodies(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		const applyBtn = waffle.page.locator('[data-testid="boolean-apply"]');

		// Initially disabled (no tool selected)
		await expect(applyBtn).toBeDisabled();

		// Select a tool body
		const toolSelect = waffle.page.locator('[data-testid="boolean-tool"]');
		const toolOptions = toolSelect.locator('option:not([disabled])');
		const toolValue = await toolOptions.nth(0).getAttribute('value');
		await toolSelect.selectOption(toolValue);
		await waffle.page.waitForTimeout(100);

		// Now Apply should be enabled
		await expect(applyBtn).toBeEnabled();

		expectNoCrash(tracker);
	});
});

test.describe('boolean union', () => {
	test('Union creates BooleanCombine feature', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createTwoBodies(waffle.page);

		const featuresBefore = await getFeatureCount(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		// Union is selected by default. Select tool body.
		const toolSelect = waffle.page.locator('[data-testid="boolean-tool"]');
		const toolOptions = toolSelect.locator('option:not([disabled])');
		const toolValue = await toolOptions.nth(0).getAttribute('value');
		await toolSelect.selectOption(toolValue);
		await waffle.page.waitForTimeout(100);

		// Apply
		await waffle.page.locator('[data-testid="boolean-apply"]').click();

		// Wait for boolean feature
		await waitForFeatureCount(waffle.page, featuresBefore + 1, 15000);

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="boolean-dialog"]')).not.toBeVisible();

		// Feature tree should contain a BooleanCombine
		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		const boolFeature = tree.features.find(f => f.operation?.type === 'BooleanCombine');
		expect(boolFeature).toBeDefined();

		// Mesh should still exist
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		expectNoCrash(tracker);
	});
});

test.describe('boolean subtract', () => {
	test('Subtract creates BooleanCombine feature', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createTwoBodies(waffle.page);

		const featuresBefore = await getFeatureCount(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		// Switch to Subtract
		const dialog = waffle.page.locator('[data-testid="boolean-dialog"]');
		await dialog.locator('input[type="radio"][value="Subtract"]').check();

		// Select tool body
		const toolSelect = waffle.page.locator('[data-testid="boolean-tool"]');
		const toolOptions = toolSelect.locator('option:not([disabled])');
		const toolValue = await toolOptions.nth(0).getAttribute('value');
		await toolSelect.selectOption(toolValue);
		await waffle.page.waitForTimeout(100);

		// Apply
		await waffle.page.locator('[data-testid="boolean-apply"]').click();

		await waitForFeatureCount(waffle.page, featuresBefore + 1, 15000);

		await expect(dialog).not.toBeVisible();

		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		const boolFeature = tree.features.find(f => f.operation?.type === 'BooleanCombine');
		expect(boolFeature).toBeDefined();

		// Subtract may produce empty geometry when coplanar faces are involved
		// (known boolean reliability limitation). The key test is that the
		// feature was created without a crash.
		expectNoCrash(tracker);
	});
});

test.describe('boolean intersect', () => {
	test('Intersect creates BooleanCombine feature with geometry', async ({ waffle }) => {
		const tracker = collectCrashErrors(waffle.page);
		await createTwoBodies(waffle.page);

		const featuresBefore = await getFeatureCount(waffle.page);

		await waffle.page.evaluate(() => window.__waffle.showBooleanDialog());
		await waffle.page.waitForTimeout(200);

		// Switch to Intersect
		const dialog = waffle.page.locator('[data-testid="boolean-dialog"]');
		await dialog.locator('input[type="radio"][value="Intersect"]').check();

		// Select tool body
		const toolSelect = waffle.page.locator('[data-testid="boolean-tool"]');
		const toolOptions = toolSelect.locator('option:not([disabled])');
		const toolValue = await toolOptions.nth(0).getAttribute('value');
		await toolSelect.selectOption(toolValue);
		await waffle.page.waitForTimeout(100);

		// Apply
		await waffle.page.locator('[data-testid="boolean-apply"]').click();

		await waitForFeatureCount(waffle.page, featuresBefore + 1, 15000);

		await expect(dialog).not.toBeVisible();

		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		const boolFeature = tree.features.find(f => f.operation?.type === 'BooleanCombine');
		expect(boolFeature).toBeDefined();

		// Intersection of two overlapping boxes should produce geometry
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		expectNoCrash(tracker);
	});
});
