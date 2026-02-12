/**
 * Revolve dialog tests — verifies the revolve workflow through GUI events.
 *
 * Pattern: sketch rectangle → finish → click Revolve → dialog interaction.
 * Mirrors the extrude.spec.js structure.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickRevolve,
	pressKey,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	getFeatureCount,
	hasFeatureOfType,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle and finish it.
 */
async function createFinishedSketch(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {}

	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('revolve-sketch-failed');
	}
}

test.describe('revolve dialog lifecycle', () => {
	test('clicking Revolve after sketch opens dialog', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickRevolve(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();
	});

	test('revolve dialog has angle input defaulting to 360', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const angleInput = waffle.page.locator('#revolve-angle');
		await expect(angleInput).toBeVisible();
		const value = await angleInput.inputValue();
		expect(parseFloat(value)).toBe(360);
	});

	test('revolve dialog Cancel closes without creating feature', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();

		// Click Cancel
		await waffle.page.locator('[data-testid="revolve-cancel"]').click();
		await waffle.page.waitForTimeout(300);

		// Dialog should be gone
		await expect(dialog).not.toBeVisible();

		// No new feature
		expect(await getFeatureCount(waffle.page)).toBe(1); // just the sketch
	});

	test('revolve dialog Escape closes without creating feature', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();

		// Press Escape
		await waffle.page.keyboard.press('Escape');
		await waffle.page.waitForTimeout(300);

		await expect(dialog).not.toBeVisible();
		expect(await getFeatureCount(waffle.page)).toBe(1);
	});

	test('revolve dialog Apply creates Revolve feature', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();

		// Set angle and click Apply
		const angleInput = waffle.page.locator('#revolve-angle');
		await angleInput.fill('180');
		await waffle.page.locator('[data-testid="revolve-apply"]').click();

		// Wait for feature creation
		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('revolve-apply-failed');
		}

		expect(await getFeatureCount(waffle.page)).toBe(2);
		expect(await hasFeatureOfType(waffle.page, 'Sketch')).toBe(true);
		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
	});

	test('revolve dialog Enter key applies', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();

		// Press Enter to apply with defaults
		await waffle.page.keyboard.press('Enter');

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('revolve-enter-failed');
		}

		expect(await hasFeatureOfType(waffle.page, 'Revolve')).toBe(true);
	});
});

test.describe('revolve dialog fields', () => {
	test('revolve dialog shows sketch name', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		// Dialog should show the sketch name
		const sketchNameEl = waffle.page.locator('#revolve-sketch');
		const name = await sketchNameEl.textContent();
		expect(name).toBeTruthy();
		expect(name.length).toBeGreaterThan(0);
	});

	test('revolve dialog has axis direction inputs', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		// Should have axis direction vector inputs
		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		const axisInputs = dialog.locator('.vec3 input');
		const count = await axisInputs.count();
		// 3 for axis origin (X/Y/Z) + 3 for axis direction (X/Y/Z)
		expect(count).toBe(6);
	});
});
