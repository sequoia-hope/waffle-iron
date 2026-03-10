/**
 * GearDialog UI tests — real toolbar/canvas interactions (no __waffle.createGear bypass).
 *
 * Tests the GearDialog form: defaults, pitch diameter update, Apply/Cancel/Escape.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, pressKey } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	getEntityCount,
	getEntityCountByType,
	waitForEntityCount,
} from './helpers/state.js';

test.describe('gear dialog UI', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('gear tool click opens dialog with defaults', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Activate gear tool via keyboard shortcut
		await pressKey(page, 'g');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'gear',
			{ timeout: 3000 }
		);

		// Click on canvas to place gear center
		await clickAt(page, 0, 0);

		// Wait for dialog to appear
		const dialog = page.locator('[data-testid="gear-dialog"]');
		await dialog.waitFor({ state: 'visible', timeout: 5000 });

		// Verify default values
		const teethInput = page.locator('[data-testid="gear-teeth-input"]');
		const pressureInput = page.locator('[data-testid="gear-pressure-input"]');
		const moduleInput = page.locator('[data-testid="gear-module-input"]');
		const pitchDiameter = page.locator('[data-testid="gear-pitch-diameter"]');

		await expect(teethInput).toBeVisible();
		await expect(pressureInput).toBeVisible();
		await expect(moduleInput).toBeVisible();
		await expect(pitchDiameter).toBeVisible();

		// Default tooth count = 20, pressure angle = 20
		expect(await teethInput.inputValue()).toBe('20');
		expect(await pressureInput.inputValue()).toBe('20');

		expectNoAnyCrash(crashes);
	});

	test('changing tooth count updates pitch diameter', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await pressKey(page, 'g');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'gear',
			{ timeout: 3000 }
		);
		await clickAt(page, 0, 0);

		const dialog = page.locator('[data-testid="gear-dialog"]');
		await dialog.waitFor({ state: 'visible', timeout: 5000 });

		// Read initial pitch diameter text
		const pitchDiameter = page.locator('[data-testid="gear-pitch-diameter"]');
		const initialText = await pitchDiameter.textContent();

		// Change tooth count to 12
		const teethInput = page.locator('[data-testid="gear-teeth-input"]');
		await teethInput.fill('12');
		// Wait for Svelte reactivity
		await page.waitForTimeout(300);

		const updatedText = await pitchDiameter.textContent();
		expect(updatedText).not.toBe(initialText);

		expectNoAnyCrash(crashes);
	});

	test('Apply creates entities and closes dialog', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await pressKey(page, 'g');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'gear',
			{ timeout: 3000 }
		);
		await clickAt(page, 0, 0);

		const dialog = page.locator('[data-testid="gear-dialog"]');
		await dialog.waitFor({ state: 'visible', timeout: 5000 });

		// Set teeth to 6 for predictable entity counts
		const teethInput = page.locator('[data-testid="gear-teeth-input"]');
		await teethInput.fill('6');
		await page.waitForTimeout(200);

		// Click Apply
		await page.locator('[data-testid="gear-apply-btn"]').click();

		// Dialog should close
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });

		// Should create entities: 12 Splines + 12 Arcs + 12 Lines + 1 Circle + points
		await waitForEntityCount(page, 20, 5000);
		const splines = await getEntityCountByType(page, 'Spline');
		const arcs = await getEntityCountByType(page, 'Arc');
		const circles = await getEntityCountByType(page, 'Circle');
		expect(splines).toBe(12);
		expect(arcs).toBe(12);
		expect(circles).toBe(1);

		expectNoAnyCrash(crashes);
	});

	test('Cancel closes dialog without entities', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await pressKey(page, 'g');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'gear',
			{ timeout: 3000 }
		);
		await clickAt(page, 0, 0);

		const dialog = page.locator('[data-testid="gear-dialog"]');
		await dialog.waitFor({ state: 'visible', timeout: 5000 });

		// Click Cancel
		await page.locator('[data-testid="gear-cancel-btn"]').click();

		// Dialog should close
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });

		// No entities should be created
		const count = await getEntityCount(page);
		expect(count).toBe(0);

		expectNoAnyCrash(crashes);
	});

	test('Escape cancels dialog', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await pressKey(page, 'g');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'gear',
			{ timeout: 3000 }
		);
		await clickAt(page, 0, 0);

		const dialog = page.locator('[data-testid="gear-dialog"]');
		await dialog.waitFor({ state: 'visible', timeout: 5000 });

		// Press Escape
		await page.keyboard.press('Escape');

		// Dialog should close
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });

		// No entities should be created
		const count = await getEntityCount(page);
		expect(count).toBe(0);

		expectNoAnyCrash(crashes);
	});
});
