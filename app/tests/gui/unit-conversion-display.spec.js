/**
 * Unit conversion display tests — StatusBar unit selector button.
 *
 * Tests the unit-selector button visibility, cycling behavior,
 * and wrap-around (mm -> cm -> m -> in -> ft -> mm).
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

test.describe('unit conversion display', () => {
	test('unit selector button is visible', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		const unitSelector = waffle.page.locator('[data-testid="unit-selector"]');
		await expect(unitSelector).toBeVisible();

		// Should have text content (a unit label like "mm")
		const text = await unitSelector.textContent();
		expect(text.trim().length).toBeGreaterThan(0);

		expectNoAnyCrash(crashes);
	});

	test('clicking cycles to next unit', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		const unitSelector = waffle.page.locator('[data-testid="unit-selector"]');
		const initialText = (await unitSelector.textContent()).trim();

		// Click to cycle
		await unitSelector.click();
		await waffle.page.waitForTimeout(200);

		const newText = (await unitSelector.textContent()).trim();
		expect(newText).not.toBe(initialText);

		expectNoAnyCrash(crashes);
	});

	test('cycling wraps around', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		const unitSelector = waffle.page.locator('[data-testid="unit-selector"]');
		const initialText = (await unitSelector.textContent()).trim();

		// UNIT_ORDER has 5 entries: mm, cm, m, in, ft
		// Click 5 times to cycle back to the start
		for (let i = 0; i < 5; i++) {
			await unitSelector.click();
			await waffle.page.waitForTimeout(150);
		}

		const afterCycleText = (await unitSelector.textContent()).trim();
		expect(afterCycleText).toBe(initialText);

		expectNoAnyCrash(crashes);
	});
});
