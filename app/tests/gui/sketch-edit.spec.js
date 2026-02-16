/**
 * Sketch editing, visibility toggle, and selection highlighting tests.
 *
 * Covers:
 * - enterSketchEditMode(): loads saved entities/positions/constraints
 * - finishSketch() edit path: sends cloned operation via editFeature() (proxy-clone bug)
 * - isSketchVisible() / toggleSketchVisibility(): visibility state
 * - Double-click sketch enters edit mode (not rename)
 * - Context menu "Edit Sketch" and "Rename" options
 * - Orange highlight on selected sketch in feature tree
 * - Visibility toggle (eye icon) in feature tree
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
	getEntityCount,
	getFeatureTree,
	getFeatureCount,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

// Selector for feature items only (excludes origin plane items)
const FEATURE_ITEM = '.tree-item:not(.origin-item)';

/**
 * Helper: create a sketch with a rectangle and finish it.
 */
async function createSketchFeature(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('edit-sketch-draw-failed');
	}
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('edit-sketch-finish-failed');
	}
}

/**
 * Helper: create sketch + extrude.
 */
async function createSketchAndExtrude(waffle) {
	await createSketchFeature(waffle);
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
		await waffle.dumpState('edit-sketch-extrude-failed');
	}
}

test.describe('sketch edit mode entry', () => {
	test('double-clicking a sketch in feature tree enters sketch edit mode', async ({ waffle }) => {
		await createSketchFeature(waffle);
		expect(await isSketchActive(waffle.page)).toBe(false);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		await sketchItem.dblclick();

		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		expect(await isSketchActive(waffle.page)).toBe(true);
	});

	test('double-clicking sketch does NOT show rename input', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		await sketchItem.dblclick();
		await waffle.page.waitForTimeout(300);

		const renameInput = waffle.page.locator('.rename-input');
		await expect(renameInput).not.toBeVisible();
	});

	test('enterSketchEditMode loads saved entities', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const sketchFeatureId = tree.features[0].id;

		await waffle.page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchFeatureId);
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		// Rectangle = 4 points + 4 lines = 8 entities
		const entityCount = await getEntityCount(waffle.page);
		expect(entityCount).toBe(8);
	});

	test('enterSketchEditMode loads saved positions', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const sketchFeatureId = tree.features[0].id;

		await waffle.page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchFeatureId);
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		const posCount = await waffle.page.evaluate(() => window.__waffle.getPositions().size);
		expect(posCount).toBeGreaterThan(0);
	});

	test('editingSketchFeatureId is set during edit mode', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const sketchFeatureId = tree.features[0].id;

		const beforeId = await waffle.page.evaluate(() => window.__waffle.getEditingSketchFeatureId());
		expect(beforeId).toBeNull();

		await waffle.page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchFeatureId);
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		const duringId = await waffle.page.evaluate(() => window.__waffle.getEditingSketchFeatureId());
		expect(duringId).toBe(sketchFeatureId);
	});

	test('context menu shows Edit Sketch for sketch features', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		await sketchItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		const contextMenu = waffle.page.locator('.context-menu');
		await expect(contextMenu).toBeVisible();

		const editBtn = contextMenu.locator('button:has-text("Edit Sketch")');
		await expect(editBtn).toBeVisible();

		const renameBtn = contextMenu.locator('button:has-text("Rename")');
		await expect(renameBtn).toBeVisible();
	});

	test('context menu Edit Sketch enters edit mode', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		await sketchItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		await waffle.page.locator('.context-menu button:has-text("Edit Sketch")').click();

		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		expect(await isSketchActive(waffle.page)).toBe(true);
	});

	test('context menu Rename for sketch shows rename input', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		await sketchItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		await waffle.page.locator('.context-menu button:has-text("Rename")').click();
		await waffle.page.waitForTimeout(200);

		const renameInput = waffle.page.locator('.rename-input');
		await expect(renameInput).toBeVisible();
	});

	test('context menu does NOT show Edit Sketch for extrude features', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await extrudeItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		const contextMenu = waffle.page.locator('.context-menu');
		await expect(contextMenu).toBeVisible();

		const editBtn = contextMenu.locator('button:has-text("Edit Sketch")');
		await expect(editBtn).not.toBeVisible();
	});
});

test.describe('sketch edit save (proxy-clone regression)', () => {
	test('editing a sketch and finishing does not throw DataCloneError', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const sketchFeatureId = tree.features[0].id;

		await waffle.page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchFeatureId);
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		// Collect any console errors during finish
		const errors = [];
		waffle.page.on('pageerror', (err) => errors.push(err.message));

		await clickFinishSketch(waffle.page);

		expect(await isSketchActive(waffle.page)).toBe(false);

		const cloneErrors = errors.filter(e => e.includes('clone') || e.includes('Proxy'));
		expect(cloneErrors).toHaveLength(0);
	});

	test('edited sketch preserves feature count (EditFeature, not AddFeature)', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const countBefore = await getFeatureCount(waffle.page);
		expect(countBefore).toBe(1);

		const tree = await getFeatureTree(waffle.page);
		const sketchFeatureId = tree.features[0].id;

		await waffle.page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchFeatureId);
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await clickFinishSketch(waffle.page);

		// Feature count should remain 1 (not 2 — we edited, not added)
		const countAfter = await getFeatureCount(waffle.page);
		expect(countAfter).toBe(1);
	});

	test('editingSketchFeatureId clears after finishing edit', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const sketchFeatureId = tree.features[0].id;

		await waffle.page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchFeatureId);
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await clickFinishSketch(waffle.page);

		const afterId = await waffle.page.evaluate(() => window.__waffle.getEditingSketchFeatureId());
		expect(afterId).toBeNull();
	});

	test('editingSketchFeatureId clears on exitSketchMode', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const sketchFeatureId = tree.features[0].id;

		await waffle.page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchFeatureId);
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		await waffle.page.evaluate(() => window.__waffle.exitSketch());
		await waffle.page.waitForTimeout(200);

		const afterId = await waffle.page.evaluate(() => window.__waffle.getEditingSketchFeatureId());
		expect(afterId).toBeNull();
	});
});

test.describe('sketch visibility toggle', () => {
	test('sketches are visible by default', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const featureId = tree.features[0].id;

		const visible = await waffle.page.evaluate(
			(id) => window.__waffle.isSketchVisible(id),
			featureId
		);
		expect(visible).toBe(true);
	});

	test('toggleSketchVisibility toggles visibility off then on', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const featureId = tree.features[0].id;

		// Toggle off
		await waffle.page.evaluate((id) => window.__waffle.toggleSketchVisibility(id), featureId);
		const afterOff = await waffle.page.evaluate(
			(id) => window.__waffle.isSketchVisible(id),
			featureId
		);
		expect(afterOff).toBe(false);

		// Toggle back on
		await waffle.page.evaluate((id) => window.__waffle.toggleSketchVisibility(id), featureId);
		const afterOn = await waffle.page.evaluate(
			(id) => window.__waffle.isSketchVisible(id),
			featureId
		);
		expect(afterOn).toBe(true);
	});

	test('eye icon is present for sketch features in feature tree', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		const visBtn = sketchItem.locator('.visibility-toggle');
		await expect(visBtn).toBeVisible();
	});

	test('eye icon is NOT present for extrude features', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		const visBtn = extrudeItem.locator('.visibility-toggle');
		await expect(visBtn).not.toBeVisible();
	});

	test('clicking eye icon toggles visibility state', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const tree = await getFeatureTree(waffle.page);
		const featureId = tree.features[0].id;

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		const visBtn = sketchItem.locator('.visibility-toggle');
		await visBtn.click();
		await waffle.page.waitForTimeout(200);

		const afterClick = await waffle.page.evaluate(
			(id) => window.__waffle.isSketchVisible(id),
			featureId
		);
		expect(afterClick).toBe(false);
	});

	test('clicking eye icon does not select the feature', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Select the extrude (second feature item)
		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await extrudeItem.click();
		await waffle.page.waitForTimeout(200);

		// Click eye icon on sketch (first feature item)
		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		const visBtn = sketchItem.locator('.visibility-toggle');
		await visBtn.click();
		await waffle.page.waitForTimeout(200);

		// Extrude should still be selected
		await expect(extrudeItem).toHaveClass(/selected/);
	});
});

test.describe('sketch selection highlight', () => {
	test('selected sketch in feature tree has orange highlight class', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).first();
		await sketchItem.click();
		await waffle.page.waitForTimeout(200);

		await expect(sketchItem).toHaveClass(/selected/);
		await expect(sketchItem).toHaveClass(/sketch-selected/);
	});

	test('selected extrude does NOT have sketch-selected class', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await extrudeItem.click();
		await waffle.page.waitForTimeout(200);

		await expect(extrudeItem).toHaveClass(/selected/);
		const classes = await extrudeItem.getAttribute('class');
		expect(classes).not.toContain('sketch-selected');
	});

	test('switching selection from sketch to extrude removes orange highlight', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const sketchItem = waffle.page.locator(FEATURE_ITEM).nth(0);
		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);

		// Select sketch
		await sketchItem.click();
		await waffle.page.waitForTimeout(200);
		await expect(sketchItem).toHaveClass(/sketch-selected/);

		// Select extrude
		await extrudeItem.click();
		await waffle.page.waitForTimeout(200);

		const sketchClasses = await sketchItem.getAttribute('class');
		expect(sketchClasses).not.toContain('sketch-selected');
		expect(sketchClasses).not.toContain('selected');
	});
});
