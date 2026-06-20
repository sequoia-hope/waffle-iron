/**
 * Planetary gear stage generator — real toolbar/dialog interactions.
 *
 * The planetary tool is a PLACEMENT tool (mirroring the single-gear tool):
 * selecting it then clicking in the sketch captures the center and opens the
 * dialog seeded with it. The dialog drives a live `planetary-preview` and, on
 * Create, adds N+2 Gear entities (sun + N planets + ring) AND N+1 center
 * Points (sun + each planet) as one undo step. Then we finish the sketch and
 * extrude. Also covers a blocking (hint-mode) invalid case and auto-adjust.
 *
 * Per project GUI rules: real interactions (no try/catch around waits),
 * crash detection via collectCrashErrors + expectNoAnyCrash.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';
import {
	isSketchActive,
	getEntityCountByType,
	getEntities,
	getActiveTool,
	getState,
	getPreview,
	waitForEntityCount,
	waitForFeatureCount,
	hasFeatureOfType,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

/**
 * Open the planetary dialog the production way: select the placement tool from
 * the toolbar, then click in the sketch at the given offset from canvas center
 * (default a non-origin spot) to seed the center and open the dialog.
 */
async function openPlanetaryDialog(page, xOffset = 60, yOffset = -40) {
	await page.locator('[data-testid="toolbar-btn-planetary"]').click();
	await clickAt(page, xOffset, yOffset);
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

	// ---- Placement tool: click-to-place center + preview + center points ----

	test('selecting the tool + clicking opens the dialog seeded with a non-origin center', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Select the placement tool — this no longer opens the dialog directly.
		await page.locator('[data-testid="toolbar-btn-planetary"]').click();
		expect(await getActiveTool(page)).toBe('planetary');
		await expect(page.locator('[data-testid="planetary-dialog"]')).toBeHidden();

		// Click in the sketch away from the origin to place the center.
		await clickAt(page, 70, -50);
		await page.locator('[data-testid="planetary-dialog"]').waitFor({ state: 'visible', timeout: 5000 });

		// Dialog is seeded with the clicked (non-origin) center.
		const st = await getState(page);
		expect(st.planetaryDialog).not.toBeNull();
		const seeded = Math.hypot(st.planetaryDialog.centerX, st.planetaryDialog.centerY);
		expect(seeded).toBeGreaterThan(1e-6);

		expectNoAnyCrash(crashes);
	});

	test('a planetary-preview appears while the dialog is open and clears on close', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openPlanetaryDialog(page);
		await page.locator('[data-testid="planetary-sun-input"]').fill('24');
		await page.locator('[data-testid="planetary-planet-input"]').fill('16');
		await page.locator('[data-testid="planetary-count-input"]').fill('4');
		await page.waitForTimeout(300);

		// A live planetary preview with one polyline per gear (N+2 = 6).
		const preview = await getPreview(page);
		expect(preview).not.toBeNull();
		expect(preview.type).toBe('planetary-preview');
		expect(Array.isArray(preview.data.polylines)).toBe(true);
		expect(preview.data.polylines.length).toBe(6);
		for (const poly of preview.data.polylines) expect(poly.length).toBeGreaterThan(2);

		// Closing the dialog clears the preview.
		await page.locator('[data-testid="planetary-cancel-btn"]').click();
		await page.locator('[data-testid="planetary-dialog"]').waitFor({ state: 'hidden', timeout: 5000 });
		await page.waitForTimeout(150);
		expect(await getPreview(page)).toBeNull();

		expectNoAnyCrash(crashes);
	});

	test('Create adds N+2 Gears and N+1 center Points, including one at the sun center', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openPlanetaryDialog(page, 70, -50);
		await page.locator('[data-testid="planetary-sun-input"]').fill('24');
		await page.locator('[data-testid="planetary-planet-input"]').fill('16');
		await page.locator('[data-testid="planetary-count-input"]').fill('4');
		await page.waitForTimeout(200);

		// Seeded sun center (== placement center) read before Create.
		const st = await getState(page);
		const sun = { x: st.planetaryDialog.centerX, y: st.planetaryDialog.centerY };
		expect(Math.hypot(sun.x, sun.y)).toBeGreaterThan(1e-6);

		const createBtn = page.locator('[data-testid="planetary-create-btn"]');
		await expect(createBtn).toBeEnabled();
		await createBtn.click();
		await page.locator('[data-testid="planetary-dialog"]').waitFor({ state: 'hidden', timeout: 5000 });

		// N+2 = 6 gears, N+1 = 5 center points (sun + 4 planets; ring shares sun).
		await waitForEntityCount(page, 6 + 5, 5000);
		expect(await getEntityCountByType(page, 'Gear')).toBe(6);
		expect(await getEntityCountByType(page, 'Point')).toBe(5);

		// A Point sits at the sun center (== the placement center).
		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const atSun = points.some(p => Math.hypot(p.x - sun.x, p.y - sun.y) < 1e-6);
		expect(atSun).toBe(true);

		expectNoAnyCrash(crashes);
	});
});
