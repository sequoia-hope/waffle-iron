/**
 * Planetary gear stage generator — real toolbar/dialog interactions.
 *
 * Opens the planetary dialog from the toolbar, sets a valid combo, Creates,
 * and asserts the active sketch gained N+2 Gear entities (sun + N planets +
 * ring). Then finishes the sketch and extrudes into a solid. Also covers a
 * blocking (hint-mode) invalid case and an auto-adjust case.
 *
 * Per project GUI rules: real interactions (no try/catch around waits),
 * crash detection via collectCrashErrors + expectNoAnyCrash.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import {
	isSketchActive,
	getEntityCountByType,
	waitForEntityCount,
	waitForFeatureCount,
	hasFeatureOfType,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

/** Open the planetary dialog via the toolbar button and wait for it. */
async function openPlanetaryDialog(page) {
	await page.locator('[data-testid="toolbar-btn-planetary"]').click();
	await page.locator('[data-testid="planetary-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
}

test.describe('planetary gear stage', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		expect(await isSketchActive(waffle.page)).toBe(true);
	});

	test('valid combo creates N+2 Gear entities', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openPlanetaryDialog(page);

		// Defaults: sun 24, planet 16, N 4 → Z_r = 56, assembly 80 % 4 == 0,
		// non-interfering. Set them explicitly so the test is robust to default
		// drift.
		await page.locator('[data-testid="planetary-sun-input"]').fill('24');
		await page.locator('[data-testid="planetary-planet-input"]').fill('16');
		await page.locator('[data-testid="planetary-count-input"]').fill('4');
		await page.waitForTimeout(200);

		// Derived ring teeth shown live.
		const ringText = await page.locator('[data-testid="planetary-ring-teeth"]').textContent();
		expect(ringText.trim()).toBe('56');

		// Create button must be enabled (valid combo, no blocking hint).
		const createBtn = page.locator('[data-testid="planetary-create-btn"]');
		await expect(createBtn).toBeEnabled();
		await createBtn.click();

		// Dialog closes; sketch gains N+2 = 6 Gear entities as one undo step.
		await page.locator('[data-testid="planetary-dialog"]').waitFor({ state: 'hidden', timeout: 5000 });
		await waitForEntityCount(page, 6, 5000);
		expect(await getEntityCountByType(page, 'Gear')).toBe(6);

		expectNoAnyCrash(crashes);
	});

	test('created stage extrudes into a solid', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openPlanetaryDialog(page);
		// Small tooth counts for a quick extrude; 12/6/3 → Z_r = 24, sum = 36,
		// 36 % 3 == 0, non-interfering.
		await page.locator('[data-testid="planetary-sun-input"]').fill('12');
		await page.locator('[data-testid="planetary-planet-input"]').fill('6');
		await page.locator('[data-testid="planetary-count-input"]').fill('3');
		await page.locator('[data-testid="planetary-module-input"]').fill('1');
		await page.waitForTimeout(200);

		const createBtn = page.locator('[data-testid="planetary-create-btn"]');
		await expect(createBtn).toBeEnabled();
		await createBtn.click();
		await page.locator('[data-testid="planetary-dialog"]').waitFor({ state: 'hidden', timeout: 5000 });

		// 3 planets + sun + ring = 5 gears.
		await waitForEntityCount(page, 5, 5000);
		expect(await getEntityCountByType(page, 'Gear')).toBe(5);

		// Finish sketch and extrude.
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('5');
		await page.locator('[data-testid="extrude-apply"]').click();

		await waitForFeatureCount(page, 2, 20000);
		expect(await hasFeatureOfType(page, 'Extrude')).toBe(true);

		expectNoAnyCrash(crashes);
	});

	test('invalid planet count blocks in hint mode', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openPlanetaryDialog(page);
		// 24/16 → sum = 80; N = 3 is NOT a divisor → assembly-invalid.
		await page.locator('[data-testid="planetary-sun-input"]').fill('24');
		await page.locator('[data-testid="planetary-planet-input"]').fill('16');
		await page.locator('[data-testid="planetary-count-input"]').fill('3');
		await page.waitForTimeout(200);

		// Hint is shown and the Create button is disabled (auto-adjust OFF).
		const hints = page.locator('[data-testid="planetary-hints"]');
		await expect(hints).toBeVisible();
		await expect(hints).toContainText('divisible');
		await expect(page.locator('[data-testid="planetary-create-btn"]')).toBeDisabled();

		// No gears created.
		expect(await getEntityCountByType(page, 'Gear')).toBe(0);

		expectNoAnyCrash(crashes);
	});

	test('auto-adjust snaps an invalid planet count and creates a stage', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openPlanetaryDialog(page);
		await page.locator('[data-testid="planetary-sun-input"]').fill('24');
		await page.locator('[data-testid="planetary-planet-input"]').fill('16');
		await page.locator('[data-testid="planetary-count-input"]').fill('3');
		// Turn auto-adjust ON — the invalid N (3) snaps to a valid divisor of 80.
		await page.locator('[data-testid="planetary-autoadjust-input"]').check();
		await page.waitForTimeout(200);

		// With auto-adjust, Create is enabled despite the hint.
		const createBtn = page.locator('[data-testid="planetary-create-btn"]');
		await expect(createBtn).toBeEnabled();
		await createBtn.click();
		await page.locator('[data-testid="planetary-dialog"]').waitFor({ state: 'hidden', timeout: 5000 });

		// A stage WAS created. The snapped N is a divisor of 80 in [1,12]
		// (2, 4, or 5), so gear count is N + 2 (sun + ring). Assert it's a valid
		// snapped count, not the invalid 3 (which would be 5 gears).
		const gearCount = await getEntityCountByType(page, 'Gear');
		expect([2 + 2, 4 + 2, 5 + 2]).toContain(gearCount);

		expectNoAnyCrash(crashes);
	});
});
