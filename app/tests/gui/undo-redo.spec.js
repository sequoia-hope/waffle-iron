/**
 * Undo/Redo tests — verifies undo/redo via toolbar buttons and keyboard shortcuts.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	pressKey,
} from './helpers/toolbar.js';
import { drawLine, drawRectangle } from './helpers/canvas.js';
import {
	getEntityCount,
	getEntities,
	getFeatureCount,
	hasFeatureOfType,
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

test.describe('undo/redo buttons', () => {
	test('undo and redo buttons are visible', async ({ waffle }) => {
		const undoBtn = waffle.page.locator('[data-testid="toolbar-btn-undo"]');
		const redoBtn = waffle.page.locator('[data-testid="toolbar-btn-redo"]');

		await expect(undoBtn).toBeVisible();
		await expect(redoBtn).toBeVisible();
	});

	test('undo button exists and is clickable', async ({ waffle }) => {
		const undoBtn = waffle.page.locator('[data-testid="toolbar-btn-undo"]');
		await expect(undoBtn).toBeVisible();
		await expect(undoBtn).toHaveText('Undo');
	});

	test('redo button exists and is clickable', async ({ waffle }) => {
		const redoBtn = waffle.page.locator('[data-testid="toolbar-btn-redo"]');
		await expect(redoBtn).toBeVisible();
		await expect(redoBtn).toHaveText('Redo');
	});
});

test.describe('undo/redo sketch operations', () => {
	test('undo after finishing sketch removes sketch feature', async ({ waffle }) => {
		// Draw and finish a sketch
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('undo-sketch-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('undo-finish-failed');
		}

		const featuresBefore = await getFeatureCount(waffle.page);
		expect(featuresBefore).toBe(1);

		// Undo via toolbar button
		await clickUndo(waffle.page);

		// Feature should be removed
		const featuresAfter = await getFeatureCount(waffle.page);
		expect(featuresAfter).toBe(0);
	});

	test('redo after undo restores sketch feature', async ({ waffle }) => {
		// Draw and finish a sketch
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('redo-sketch-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('redo-sketch-finish-failed');
		}

		// Undo
		await clickUndo(waffle.page);
		expect(await getFeatureCount(waffle.page)).toBe(0);

		// Redo — feature should come back
		await clickRedo(waffle.page);

		// Wait for feature to reappear
		try {
			await waitForFeatureCount(waffle.page, 1, 5000);
		} catch {
			await waffle.dumpState('redo-sketch-restore-failed');
		}

		expect(await getFeatureCount(waffle.page)).toBe(1);
		expect(await hasFeatureOfType(waffle.page, 'Sketch')).toBe(true);
	});
});

test.describe('undo/redo extrude operations', () => {
	test('undo extrude removes extrude feature but keeps sketch', async ({ waffle }) => {
		// Full sketch + extrude workflow
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('undo-extrude-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('undo-extrude-finish-failed');
		}

		await clickExtrude(waffle.page);
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('undo-extrude-apply-failed');
		}

		expect(await getFeatureCount(waffle.page)).toBe(2);
		expect(await hasFeatureOfType(waffle.page, 'Extrude')).toBe(true);

		// Undo the extrude
		await clickUndo(waffle.page);

		// Should have 1 feature (sketch), not 2
		expect(await getFeatureCount(waffle.page)).toBe(1);
		expect(await hasFeatureOfType(waffle.page, 'Sketch')).toBe(true);
		expect(await hasFeatureOfType(waffle.page, 'Extrude')).toBe(false);
	});

	test('redo extrude after undo restores it', async ({ waffle }) => {
		// Full sketch + extrude
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('redo-extrude-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('redo-extrude-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('redo-extrude-apply-failed');
		}

		// Undo then redo
		await clickUndo(waffle.page);
		expect(await getFeatureCount(waffle.page)).toBe(1);

		await clickRedo(waffle.page);
		try { await waitForFeatureCount(waffle.page, 2, 5000); } catch {
			await waffle.dumpState('redo-extrude-restore-failed');
		}

		expect(await getFeatureCount(waffle.page)).toBe(2);
		expect(await hasFeatureOfType(waffle.page, 'Extrude')).toBe(true);
	});
});

test.describe('undo/redo keyboard shortcuts', () => {
	test('Ctrl+Z triggers undo', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('ctrlz-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('ctrlz-finish-failed');
		}

		// Ctrl+Z to undo
		await waffle.page.keyboard.press('Control+z');
		await waffle.page.waitForTimeout(500);

		expect(await getFeatureCount(waffle.page)).toBe(0);
	});

	test('Ctrl+Shift+Z triggers redo', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('ctrlsz-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('ctrlsz-finish-failed');
		}

		// Undo via Ctrl+Z
		await waffle.page.keyboard.press('Control+z');
		await waffle.page.waitForTimeout(500);
		expect(await getFeatureCount(waffle.page)).toBe(0);

		// Redo via Ctrl+Shift+Z
		await waffle.page.keyboard.press('Control+Shift+z');
		await waffle.page.waitForTimeout(500);

		try { await waitForFeatureCount(waffle.page, 1, 5000); } catch {
			await waffle.dumpState('ctrlsz-restore-failed');
		}
		expect(await getFeatureCount(waffle.page)).toBe(1);
	});
});
