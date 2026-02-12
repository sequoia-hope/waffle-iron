/**
 * Advanced undo/redo tests — edge cases for undo/redo with empty history,
 * tree item count consistency, and redo stack clearing behavior.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	getFeatureCount,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

/**
 * Helper: click the Undo toolbar button.
 */
async function clickUndo(page) {
	await page.locator('[data-testid="toolbar-btn-undo"]').click();
	await page.waitForTimeout(500);
}

/**
 * Helper: click the Redo toolbar button.
 */
async function clickRedo(page) {
	await page.locator('[data-testid="toolbar-btn-redo"]').click();
	await page.waitForTimeout(500);
}

/**
 * Helper: create a finished sketch with a rectangle (1 feature).
 */
async function createFinishedSketch(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('undo-adv-draw-failed');
	}

	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('undo-adv-finish-failed');
	}
}

test.describe('undo/redo edge cases', () => {
	test('undo with empty history is a no-op', async ({ waffle }) => {
		// Fresh state — no features
		const countBefore = await getFeatureCount(waffle.page);
		expect(countBefore).toBe(0);

		// Click undo with nothing to undo
		await clickUndo(waffle.page);

		// Feature count should still be 0
		const countAfter = await getFeatureCount(waffle.page);
		expect(countAfter).toBe(0);

		// Canvas should still be visible (no crash)
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('redo with empty redo stack is a no-op', async ({ waffle }) => {
		// Fresh state — no features, no redo history
		const countBefore = await getFeatureCount(waffle.page);
		expect(countBefore).toBe(0);

		// Click redo with nothing to redo
		await clickRedo(waffle.page);

		const countAfter = await getFeatureCount(waffle.page);
		expect(countAfter).toBe(0);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('feature tree item count matches after undo then redo', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		// Should have 1 tree item
		const treeItemsAfterCreate = waffle.page.locator('.tree-item');
		await expect(treeItemsAfterCreate).toHaveCount(1);

		// Undo — tree items should drop to 0
		await clickUndo(waffle.page);
		const treeItemsAfterUndo = waffle.page.locator('.tree-item');
		await expect(treeItemsAfterUndo).toHaveCount(0);

		// Redo — tree items should return to 1
		await clickRedo(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 5000);
		} catch {
			await waffle.dumpState('undo-adv-redo-failed');
		}

		const treeItemsAfterRedo = waffle.page.locator('.tree-item');
		await expect(treeItemsAfterRedo).toHaveCount(1);
	});

	test('new action after undo clears redo stack', async ({ waffle }) => {
		// Create first sketch (1 feature)
		await createFinishedSketch(waffle);
		expect(await getFeatureCount(waffle.page)).toBe(1);

		// Undo (0 features, 1 on redo stack)
		await clickUndo(waffle.page);
		expect(await getFeatureCount(waffle.page)).toBe(0);

		// Create a new sketch (new action should clear redo stack)
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -60, -40, 60, 40);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('undo-adv-new-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('undo-adv-new-finish-failed');
		}

		expect(await getFeatureCount(waffle.page)).toBe(1);

		// Redo should be a no-op (redo stack was cleared by new action)
		await clickRedo(waffle.page);

		const countAfterRedo = await getFeatureCount(waffle.page);
		expect(countAfterRedo).toBe(1);
	});
});
