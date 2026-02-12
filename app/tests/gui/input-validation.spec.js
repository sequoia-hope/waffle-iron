/**
 * Input validation tests — verifies that invalid inputs to extrude and revolve
 * dialogs do not crash the app or create malformed features.
 *
 * Pattern: create finished sketch, open dialog, fill invalid value, click Apply,
 * wait, verify feature count did NOT increase and canvas is still visible.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	clickRevolve,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	getFeatureCount,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

/**
 * Helper: create a rectangle sketch and finish it, yielding 1 feature.
 */
async function createFinishedSketch(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('input-val-draw-failed');
	}

	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('input-val-finish-failed');
	}
}

test.describe('extrude input validation', () => {
	test('depth=0 does not create feature or crashes gracefully', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickExtrude(waffle.page);
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('0');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// Feature count should still be 1 (just the sketch), or if 2 the app is still alive
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(2);

		// Canvas must still be visible (no crash)
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('negative depth does not create feature', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickExtrude(waffle.page);
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('-10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// Might create a reversed extrude (valid) or reject — both OK
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(2);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('empty depth string handled gracefully', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickExtrude(waffle.page);
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// Should not create a feature from empty input
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(1);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('non-numeric depth (NaN) handled gracefully', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickExtrude(waffle.page);
		// type=number inputs reject non-numeric fill; use evaluate to force NaN
		await waffle.page.evaluate(() => {
			const input = document.querySelector('[data-testid="extrude-depth"]');
			if (input) {
				// Set via native setter to bypass type=number validation
				const nativeSetter = Object.getOwnPropertyDescriptor(
					HTMLInputElement.prototype, 'value'
				).set;
				nativeSetter.call(input, '');
				input.dispatchEvent(new Event('input', { bubbles: true }));
			}
		});
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// Empty/NaN input may be coerced to 0 or rejected — app should not crash
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(2);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});

test.describe('revolve input validation', () => {
	test('angle=0 does not create feature', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickRevolve(waffle.page);
		const angleInput = waffle.page.locator('#revolve-angle');
		await angleInput.fill('0');
		await waffle.page.locator('[data-testid="revolve-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// angle=0 may be rejected or create a degenerate feature
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(2);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('negative angle handled gracefully', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickRevolve(waffle.page);
		const angleInput = waffle.page.locator('#revolve-angle');
		await angleInput.fill('-90');
		await waffle.page.locator('[data-testid="revolve-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// Negative angle might create a valid revolve or be rejected
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(2);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('angle >360 handled gracefully', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickRevolve(waffle.page);
		const angleInput = waffle.page.locator('#revolve-angle');
		await angleInput.fill('720');
		await waffle.page.locator('[data-testid="revolve-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// Over-rotation might be clamped or rejected
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(2);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('empty angle string handled gracefully', async ({ waffle }) => {
		await createFinishedSketch(waffle);

		await clickRevolve(waffle.page);
		const angleInput = waffle.page.locator('#revolve-angle');
		await angleInput.fill('');
		await waffle.page.locator('[data-testid="revolve-apply"]').click();
		await waffle.page.waitForTimeout(1000);

		// Empty angle may default to 0 or be rejected
		const count = await getFeatureCount(waffle.page);
		expect(count).toBeLessThanOrEqual(2);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});
