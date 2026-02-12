/**
 * Feature tree interaction tests — verifies clicking, selecting, renaming,
 * and context menu operations on the feature tree panel.
 *
 * Uses real DOM clicks on feature tree items.
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
	getFeatureCount,
	getFeatureTree,
	hasFeatureOfType,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

/**
 * Helper: create a sketch and finish it.
 */
async function createSketchFeature(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('ft-sketch-draw-failed');
	}

	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('ft-sketch-failed');
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
		await waffle.dumpState('ft-extrude-failed');
	}
}

test.describe('feature tree display', () => {
	test('feature tree panel is visible', async ({ waffle }) => {
		const panel = waffle.page.locator('.feature-tree');
		await expect(panel).toBeVisible();
	});

	test('empty feature tree shows "No features yet"', async ({ waffle }) => {
		const emptyState = waffle.page.locator('.feature-tree .empty-state');
		await expect(emptyState).toBeVisible();
		await expect(emptyState).toHaveText('No features yet');
	});

	test('feature tree shows sketch after finishing sketch', async ({ waffle }) => {
		await createSketchFeature(waffle);

		// Should show one tree item
		const treeItems = waffle.page.locator('.tree-item');
		await expect(treeItems).toHaveCount(1);

		// Item should contain a label with the feature name
		const label = treeItems.first().locator('.tree-label');
		const text = await label.textContent();
		expect(text).toBeTruthy();
		expect(text.length).toBeGreaterThan(0);
	});

	test('feature tree shows sketch and extrude after full workflow', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const treeItems = waffle.page.locator('.tree-item');
		await expect(treeItems).toHaveCount(2);

		// First should be sketch, second should be extrude
		const labels = await treeItems.locator('.tree-label').allTextContents();
		expect(labels).toHaveLength(2);
	});
});

test.describe('feature tree selection', () => {
	test('clicking a feature in the tree selects it', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const treeItem = waffle.page.locator('.tree-item').first();

		// Click the feature
		await treeItem.click();
		await waffle.page.waitForTimeout(200);

		// Should have .selected class
		await expect(treeItem).toHaveClass(/selected/);
	});

	test('clicking a different feature changes selection', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const items = waffle.page.locator('.tree-item');
		const firstItem = items.nth(0);
		const secondItem = items.nth(1);

		// Click first
		await firstItem.click();
		await waffle.page.waitForTimeout(200);
		await expect(firstItem).toHaveClass(/selected/);

		// Click second
		await secondItem.click();
		await waffle.page.waitForTimeout(200);
		await expect(secondItem).toHaveClass(/selected/);
		// First should no longer be selected
		const firstClasses = await firstItem.getAttribute('class');
		expect(firstClasses).not.toContain('selected');
	});
});

test.describe('feature tree context menu', () => {
	test('right-click on feature opens context menu', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const treeItem = waffle.page.locator('.tree-item').first();
		await treeItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		// Context menu should appear
		const contextMenu = waffle.page.locator('.context-menu');
		await expect(contextMenu).toBeVisible();

		// Should have Suppress and Delete options
		const suppressBtn = contextMenu.locator('text=Suppress');
		const deleteBtn = contextMenu.locator('text=Delete');
		await expect(suppressBtn).toBeVisible();
		await expect(deleteBtn).toBeVisible();
	});

	test('clicking away closes context menu', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const treeItem = waffle.page.locator('.tree-item').first();
		await treeItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		const contextMenu = waffle.page.locator('.context-menu');
		await expect(contextMenu).toBeVisible();

		// Click elsewhere to close
		await waffle.page.locator('.feature-tree .panel-header').click();
		await waffle.page.waitForTimeout(200);

		await expect(contextMenu).not.toBeVisible();
	});

	test('suppress via context menu dims the feature', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const firstItem = waffle.page.locator('.tree-item').first();
		await firstItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		// Click Suppress
		await waffle.page.locator('.context-menu .ctx-item >> text=Suppress').click();
		await waffle.page.waitForTimeout(500);

		// Feature should have .suppressed class
		await expect(firstItem).toHaveClass(/suppressed/);
	});

	test('delete via context menu removes the feature', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const itemsBefore = await waffle.page.locator('.tree-item').count();
		expect(itemsBefore).toBe(2);

		// Right-click the second feature (extrude) and delete
		const secondItem = waffle.page.locator('.tree-item').nth(1);
		await secondItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		await waffle.page.locator('.context-menu .ctx-item.danger >> text=Delete').click();
		await waffle.page.waitForTimeout(500);

		// Should have one fewer feature
		const itemsAfter = await waffle.page.locator('.tree-item').count();
		expect(itemsAfter).toBe(1);
	});
});

test.describe('feature tree rename', () => {
	test('double-clicking feature shows rename input', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const treeItem = waffle.page.locator('.tree-item').first();
		await treeItem.dblclick();
		await waffle.page.waitForTimeout(200);

		// Rename input should appear
		const renameInput = waffle.page.locator('.rename-input');
		await expect(renameInput).toBeVisible();
	});

	test('typing new name and pressing Enter renames feature', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const treeItem = waffle.page.locator('.tree-item').first();
		await treeItem.dblclick();
		await waffle.page.waitForTimeout(200);

		const renameInput = waffle.page.locator('.rename-input');
		await renameInput.fill('My Cool Sketch');
		await waffle.page.keyboard.press('Enter');
		await waffle.page.waitForTimeout(200);

		// Label should show new name
		const label = treeItem.locator('.tree-label');
		await expect(label).toHaveText('My Cool Sketch');
	});

	test('pressing Escape cancels rename', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const treeItem = waffle.page.locator('.tree-item').first();
		const labelBefore = await treeItem.locator('.tree-label').textContent();

		await treeItem.dblclick();
		await waffle.page.waitForTimeout(200);

		const renameInput = waffle.page.locator('.rename-input');
		await renameInput.fill('Some Other Name');
		await waffle.page.keyboard.press('Escape');
		await waffle.page.waitForTimeout(200);

		// Label should revert to original name
		const labelAfter = await treeItem.locator('.tree-label').textContent();
		expect(labelAfter).toBe(labelBefore);
	});
});

test.describe('feature tree rollback slider', () => {
	test('rollback slider appears when features exist', async ({ waffle }) => {
		await createSketchFeature(waffle);

		const slider = waffle.page.locator('.rollback-slider');
		await expect(slider).toBeVisible();
	});

	test('rollback slider not present when no features', async ({ waffle }) => {
		const slider = waffle.page.locator('.rollback-slider');
		await expect(slider).not.toBeVisible();
	});

	test('rollback slider max matches feature count', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const slider = waffle.page.locator('.rollback-slider');
		const max = await slider.getAttribute('max');
		expect(parseInt(max)).toBe(2);
	});
});
