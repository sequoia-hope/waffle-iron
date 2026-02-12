/**
 * Error path tests — verifies that the app handles edge cases and
 * out-of-order actions gracefully without crashing.
 *
 * Each test verifies the canvas remains visible after the problematic action.
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
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

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
		await waffle.dumpState('error-paths-draw-failed');
	}

	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('error-paths-finish-failed');
	}
}

test.describe('operations without prerequisites', () => {
	test('clicking Extrude with no sketch does not crash', async ({ waffle }) => {
		// Fresh state — no sketch exists
		const countBefore = await getFeatureCount(waffle.page);
		expect(countBefore).toBe(0);

		// Try clicking the extrude button (may not open dialog without a sketch)
		const extrudeBtn = waffle.page.locator('[data-testid="toolbar-btn-extrude"]');
		const isVisible = await extrudeBtn.isVisible();
		if (isVisible) {
			try {
				await extrudeBtn.click();
				await waffle.page.waitForTimeout(500);
			} catch {
				// Button click may fail if disabled — that is acceptable
			}
		}

		// App should not crash
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();

		const countAfter = await getFeatureCount(waffle.page);
		expect(countAfter).toBe(0);
	});

	test('clicking Revolve with no sketch does not crash', async ({ waffle }) => {
		const countBefore = await getFeatureCount(waffle.page);
		expect(countBefore).toBe(0);

		const revolveBtn = waffle.page.locator('[data-testid="toolbar-btn-revolve"]');
		const isVisible = await revolveBtn.isVisible();
		if (isVisible) {
			try {
				await revolveBtn.click();
				await waffle.page.waitForTimeout(500);
			} catch {
				// Button click may fail if disabled
			}
		}

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();

		const countAfter = await getFeatureCount(waffle.page);
		expect(countAfter).toBe(0);
	});

	test('Finish Sketch with no entities creates feature or rejects gracefully', async ({ waffle }) => {
		// Enter sketch mode but draw nothing
		await clickSketch(waffle.page);

		// Immediately finish — no entities drawn
		try {
			await clickFinishSketch(waffle.page);
		} catch {
			// May throw if finish button is not visible or rejects empty sketch
		}
		await waffle.page.waitForTimeout(500);

		// Feature count should be 0 (rejected empty sketch) or 1 (accepted empty)
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(1);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});

test.describe('out-of-order actions', () => {
	test('double-clicking Sketch button is safe', async ({ waffle }) => {
		// Enter sketch mode
		await clickSketch(waffle.page);

		// Check if sketch button is still visible (it may be hidden in sketch mode)
		const sketchBtn = waffle.page.locator('[data-testid="toolbar-btn-sketch"]');
		const isVisible = await sketchBtn.isVisible();
		if (isVisible) {
			await sketchBtn.click({ timeout: 2000 });
			await waffle.page.waitForTimeout(300);
		}

		// App should not crash
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('pressing Escape 5 times rapidly does not crash', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Press Escape rapidly 5 times
		for (let i = 0; i < 5; i++) {
			await waffle.page.keyboard.press('Escape');
			await waffle.page.waitForTimeout(50);
		}

		await waffle.page.waitForTimeout(300);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('pressing L/R/C keys while extrude dialog is open', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickExtrude(waffle.page);

		// Press sketch tool shortcut keys while dialog is open
		await waffle.page.keyboard.press('l');
		await waffle.page.waitForTimeout(100);
		await waffle.page.keyboard.press('r');
		await waffle.page.waitForTimeout(100);
		await waffle.page.keyboard.press('c');
		await waffle.page.waitForTimeout(100);

		// Extrude dialog should still be visible (keys should not close it)
		const extrudeDialog = waffle.page.locator('[data-testid="extrude-dialog"]');
		await expect(extrudeDialog).toBeVisible();

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('clicking Extrude while already in extrude dialog', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickExtrude(waffle.page);

		// Verify dialog is open
		const extrudeDialog = waffle.page.locator('[data-testid="extrude-dialog"]');
		await expect(extrudeDialog).toBeVisible();

		// The dialog overlay intercepts pointer events — use force click to bypass
		const extrudeBtn = waffle.page.locator('[data-testid="toolbar-btn-extrude"]');
		const isVisible = await extrudeBtn.isVisible();
		if (isVisible) {
			try {
				await extrudeBtn.click({ force: true, timeout: 2000 });
			} catch {
				// Force click may still fail — that's OK
			}
			await waffle.page.waitForTimeout(500);
		}

		// App should not crash — dialog may be visible or closed
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});
