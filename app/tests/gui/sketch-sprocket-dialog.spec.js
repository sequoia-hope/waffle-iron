/**
 * SprocketDialog UI tests — real toolbar/canvas interactions (no
 * __waffle.createSprocket bypass). B3 checkpoint 2 of
 * `specs/custom_features_and_modeling_roadmap.md`.
 *
 * Tests the sprocket placement tool (K) and dialog: defaults from the ISO 08B
 * preset, preset switching, pitch-diameter update, the engine's typed refusal
 * disabling Apply, Apply/Cancel/Escape, and double-click edit.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, pressKey } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	getEntityCount,
	getEntityCountByType,
	getEntities,
	waitForEntityCount,
} from './helpers/state.js';

async function openSprocketDialog(page) {
	await pressKey(page, 'k');
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'sprocket',
		{ timeout: 3000 }
	);
	await clickAt(page, 0, 0);
	const dialog = page.locator('[data-testid="sprocket-dialog"]');
	await dialog.waitFor({ state: 'visible', timeout: 5000 });
	return dialog;
}

test.describe('sprocket dialog UI', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('sprocket tool click opens dialog with ISO 08B defaults', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openSprocketDialog(page);

		const teethInput = page.locator('[data-testid="sprocket-teeth-input"]');
		const chainSelect = page.locator('[data-testid="sprocket-chain-select"]');
		const pitchInput = page.locator('[data-testid="sprocket-pitch-input"]');
		const rollerInput = page.locator('[data-testid="sprocket-roller-input"]');
		const pitchDiameter = page.locator('[data-testid="sprocket-pitch-diameter"]');

		await expect(teethInput).toBeVisible();
		await expect(chainSelect).toBeVisible();
		await expect(pitchInput).toBeVisible();
		await expect(rollerInput).toBeVisible();
		await expect(pitchDiameter).toBeVisible();

		expect(await teethInput.inputValue()).toBe('20');
		expect(await chainSelect.inputValue()).toBe('08B');
		// Display unit is mm by default: 12.7 mm pitch, 8.51 mm roller.
		expect(await pitchInput.inputValue()).toBe('12.7');
		expect(await rollerInput.inputValue()).toBe('8.51');
		// d = p / sin(π/z) = 12.7 / sin(9°) ≈ 81.18 mm
		expect(await pitchDiameter.textContent()).toContain('81.18');

		// Apply is enabled: the engine accepted the default parameters.
		await expect(page.locator('[data-testid="sprocket-apply-btn"]')).toBeEnabled();
		await expect(page.locator('[data-testid="sprocket-error"]')).toHaveCount(0);

		expectNoAnyCrash(crashes);
	});

	test('changing the chain preset reseeds pitch and roller; hand edits become Custom', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openSprocketDialog(page);
		const chainSelect = page.locator('[data-testid="sprocket-chain-select"]');
		const pitchInput = page.locator('[data-testid="sprocket-pitch-input"]');
		const rollerInput = page.locator('[data-testid="sprocket-roller-input"]');
		const pitchDiameter = page.locator('[data-testid="sprocket-pitch-diameter"]');
		const before = await pitchDiameter.textContent();

		await chainSelect.selectOption('10B');
		await page.waitForTimeout(200);
		expect(await pitchInput.inputValue()).toBe('15.875');
		expect(await rollerInput.inputValue()).toBe('10.16');
		expect(await pitchDiameter.textContent()).not.toBe(before);

		// Editing the roller by hand moves the preset to Custom...
		await rollerInput.fill('10');
		await page.waitForTimeout(200);
		expect(await chainSelect.inputValue()).toBe('custom');
		// ...and restoring it snaps back to the matching preset.
		await rollerInput.fill('10.16');
		await page.waitForTimeout(200);
		expect(await chainSelect.inputValue()).toBe('10B');

		expectNoAnyCrash(crashes);
	});

	test('changing tooth count updates pitch diameter', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openSprocketDialog(page);
		const pitchDiameter = page.locator('[data-testid="sprocket-pitch-diameter"]');
		const initialText = await pitchDiameter.textContent();

		await page.locator('[data-testid="sprocket-teeth-input"]').fill('12');
		await page.waitForTimeout(300);

		const updatedText = await pitchDiameter.textContent();
		expect(updatedText).not.toBe(initialText);
		// 12.7 / sin(15°) ≈ 49.07 mm
		expect(updatedText).toContain('49.07');

		expectNoAnyCrash(crashes);
	});

	test('the engine refusal is shown and disables Apply', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openSprocketDialog(page);
		const applyBtn = page.locator('[data-testid="sprocket-apply-btn"]');
		await expect(applyBtn).toBeEnabled();

		// Too few teeth: the generator's typed refusal names the value.
		await page.locator('[data-testid="sprocket-teeth-input"]').fill('3');
		const error = page.locator('[data-testid="sprocket-error"]');
		await error.waitFor({ state: 'visible', timeout: 5000 });
		expect(await error.textContent()).toMatch(/tooth_count 3/);
		await expect(applyBtn).toBeDisabled();

		// Enter must not create anything while refused.
		await page.keyboard.press('Enter');
		await page.waitForTimeout(300);
		await expect(page.locator('[data-testid="sprocket-dialog"]')).toBeVisible();
		expect(await getEntityCount(page)).toBe(0);

		// Back to a valid count: the refusal clears and Apply re-enables.
		await page.locator('[data-testid="sprocket-teeth-input"]').fill('9');
		await expect(error).toHaveCount(0, { timeout: 5000 });
		await expect(applyBtn).toBeEnabled();

		expectNoAnyCrash(crashes);
	});

	test('Apply creates one Sprocket entity and closes dialog', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		const dialog = await openSprocketDialog(page);
		await page.locator('[data-testid="sprocket-teeth-input"]').fill('12');
		await page.waitForTimeout(200);

		await page.locator('[data-testid="sprocket-apply-btn"]').click();
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });

		// One compact Sprocket entity carrying the dialog's params (metres)...
		await waitForEntityCount(page, 1, 5000);
		expect(await getEntityCountByType(page, 'Sprocket')).toBe(1);
		const entities = await getEntities(page);
		const s = entities.find((e) => e.type === 'Sprocket');
		expect(s.params.toothCount).toBe(12);
		expect(s.params.pitch).toBeCloseTo(0.0127, 9);
		expect(s.params.rollerDiameter).toBeCloseTo(0.00851, 9);

		// ...and its display expansion: 4 arcs per tooth + the pitch circle.
		const counts = await page.evaluate(() => {
			const disp = window.__waffle.getGearDisplay();
			return Object.values(disp)[0]?.counts ?? {};
		});
		expect(counts.Arc).toBe(48);
		expect(counts.Circle).toBe(1);
		expect(counts.Spline ?? 0).toBe(0);

		expectNoAnyCrash(crashes);
	});

	test('Cancel closes dialog without entities', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		const dialog = await openSprocketDialog(page);
		await page.locator('[data-testid="sprocket-cancel-btn"]').click();
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });
		expect(await getEntityCount(page)).toBe(0);

		expectNoAnyCrash(crashes);
	});

	test('Escape cancels dialog', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		const dialog = await openSprocketDialog(page);
		await page.keyboard.press('Escape');
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });
		expect(await getEntityCount(page)).toBe(0);

		expectNoAnyCrash(crashes);
	});

	test('double-click on a sprocket opens the edit dialog and Apply updates it in place', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		const dialog = await openSprocketDialog(page);
		await page.locator('[data-testid="sprocket-apply-btn"]').click();
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });
		await waitForEntityCount(page, 1, 5000);
		const beforeId = (await getEntities(page)).find((e) => e.type === 'Sprocket').id;

		// Select tool, then double-click inside the sprocket (its centre is
		// at the canvas centre; the outline hit-test covers the interior).
		await page.keyboard.press('Escape');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'select',
			{ timeout: 3000 }
		);
		await clickAt(page, 0, 0);
		await clickAt(page, 0, 0);
		await dialog.waitFor({ state: 'visible', timeout: 5000 });
		await expect(page.locator('.dialog-title')).toContainText('Edit Sprocket');
		expect(await page.locator('[data-testid="sprocket-teeth-input"]').inputValue()).toBe('20');

		await page.locator('[data-testid="sprocket-teeth-input"]').fill('16');
		await page.waitForTimeout(200);
		await page.locator('[data-testid="sprocket-apply-btn"]').click();
		await dialog.waitFor({ state: 'hidden', timeout: 5000 });

		// Still one sprocket (edited, not duplicated), now 16 teeth.
		await waitForEntityCount(page, 1, 5000);
		const after = (await getEntities(page)).find((e) => e.type === 'Sprocket');
		expect(after.params.toothCount).toBe(16);
		expect(after.id).not.toBe(beforeId); // the compact entity is re-minted on edit, like a gear
		const counts = await page.evaluate(() => {
			const disp = window.__waffle.getGearDisplay();
			return Object.values(disp)[0]?.counts ?? {};
		});
		expect(counts.Arc).toBe(64);

		expectNoAnyCrash(crashes);
	});
});
