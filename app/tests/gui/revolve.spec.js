/**
 * Revolve dialog tests — verifies the revolve workflow through GUI events.
 *
 * Pattern: sketch rectangle → finish → click Revolve → dialog interaction.
 * Mirrors the extrude.spec.js structure.
 */
import { test, expect } from './helpers/waffle-test.js';
import { pickOffsetRevolveAxis } from './helpers/revolve.js';
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
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('revolve-sketch-draw-failed');
	}

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
		await pickOffsetRevolveAxis(waffle.page);

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
		await pickOffsetRevolveAxis(waffle.page);

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

test.describe('revolve dialog angle validation', () => {
	test('angle input accepts valid values between 0.1 and 360', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const angleInput = waffle.page.locator('#revolve-angle');

		// Set to 90 — should accept
		await angleInput.fill('90');
		expect(await angleInput.inputValue()).toBe('90');

		// Set to 270 — should accept
		await angleInput.fill('270');
		expect(await angleInput.inputValue()).toBe('270');
	});

	test('angle input has min=0.1 and max=360 attributes', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const angleInput = waffle.page.locator('#revolve-angle');
		const min = await angleInput.getAttribute('min');
		const max = await angleInput.getAttribute('max');
		expect(parseFloat(min)).toBeLessThanOrEqual(1);
		expect(parseFloat(max)).toBe(360);
	});

	test('Apply disabled when no axis is selected', async ({ waffle }) => {
		// Since the March 2026 revolve UX overhaul the axis is an explicit
		// viewport pick — the dialog opens with NO axis and Apply disabled.
		// (The old auto-select-first-line behavior this test once asserted
		// is gone.)
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();

		const applyBtn = waffle.page.locator('[data-testid="revolve-apply"]');
		await expect(applyBtn).toBeDisabled();

		// Setting an axis (test API = a viewport pick) enables it.
		await pickOffsetRevolveAxis(waffle.page);
		await expect(applyBtn).toBeEnabled();
	});
});

test.describe('revolve dialog state after apply', () => {
	test('dialog closes after successful apply', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);
		await pickOffsetRevolveAxis(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();

		// Apply with default angle
		await waffle.page.locator('[data-testid="revolve-apply"]').click();

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('revolve-close-after-apply');
		}

		// Dialog should be closed after apply
		await expect(dialog).not.toBeVisible();
	});

	test('feature tree updates with Revolve after apply', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);
		await pickOffsetRevolveAxis(waffle.page);

		await waffle.page.locator('#revolve-angle').fill('180');
		await waffle.page.locator('[data-testid="revolve-apply"]').click();

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('revolve-tree-update');
		}

		// Verify feature tree contains both Sketch and Revolve
		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		expect(tree.features.length).toBe(2);
		expect(tree.features[0].operation.type).toBe('Sketch');
		expect(tree.features[1].operation.type).toBe('Revolve');
	});

	test('revolve dialog close button (X) closes dialog', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		const dialog = waffle.page.locator('[data-testid="revolve-dialog"]');
		await expect(dialog).toBeVisible();

		// Click the X close button in dialog header
		await dialog.locator('.close-btn').click();
		await waffle.page.waitForTimeout(300);

		await expect(dialog).not.toBeVisible();
		expect(await getFeatureCount(waffle.page)).toBe(1);
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

	test('revolve dialog has axis pick box', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		// Should have axis pick box (viewport-based picking)
		const axisBox = waffle.page.locator('[data-testid="revolve-axis-box"]');
		await expect(axisBox).toBeVisible();

		// Axis pick mode should be active by default
		const isAxisActive = await waffle.page.evaluate(
			() => window.__waffle?.getAxisPickMode?.()
		);
		expect(isAxisActive).toBe(true);
	});

	test('revolve dialog has profile pick box', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await clickRevolve(waffle.page);

		// Should have profile pick box
		const profileBox = waffle.page.locator('[data-testid="revolve-profile-box"]');
		await expect(profileBox).toBeVisible();

		// Default profile should be selected
		const profileItem = waffle.page.locator('[data-testid="revolve-profile-item"]');
		await expect(profileItem).toBeVisible();
	});
});
