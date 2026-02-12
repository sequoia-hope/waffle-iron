/**
 * Modeling operation button tests — verifies fillet, chamfer, and shell
 * toolbar buttons exist and are accessible.
 *
 * Note: Fillet/Chamfer/Shell dialogs do NOT exist yet (only ExtrudeDialog
 * and RevolveDialog are implemented). These tests verify the buttons are
 * present and that clicking them doesn't crash the application.
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
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

test.describe('modeling operation buttons', () => {
	test('fillet button is visible in modeling mode', async ({ waffle }) => {
		const btn = waffle.page.locator('[data-testid="toolbar-btn-fillet"]');
		await expect(btn).toBeVisible();
	});

	test('chamfer button is visible in modeling mode', async ({ waffle }) => {
		const btn = waffle.page.locator('[data-testid="toolbar-btn-chamfer"]');
		await expect(btn).toBeVisible();
	});

	test('shell button is visible in modeling mode', async ({ waffle }) => {
		const btn = waffle.page.locator('[data-testid="toolbar-btn-shell"]');
		await expect(btn).toBeVisible();
	});

	test('fillet button not visible in sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const btn = waffle.page.locator('[data-testid="toolbar-btn-fillet"]');
		await expect(btn).not.toBeVisible();
	});

	test('chamfer button not visible in sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const btn = waffle.page.locator('[data-testid="toolbar-btn-chamfer"]');
		await expect(btn).not.toBeVisible();
	});

	test('shell button not visible in sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const btn = waffle.page.locator('[data-testid="toolbar-btn-shell"]');
		await expect(btn).not.toBeVisible();
	});

	test('clicking fillet button does not crash the app', async ({ waffle }) => {
		const btn = waffle.page.locator('[data-testid="toolbar-btn-fillet"]');
		await btn.click();
		await waffle.page.waitForTimeout(500);

		// App should still be alive — canvas visible
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('clicking chamfer button does not crash the app', async ({ waffle }) => {
		const btn = waffle.page.locator('[data-testid="toolbar-btn-chamfer"]');
		await btn.click();
		await waffle.page.waitForTimeout(500);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('clicking shell button does not crash the app', async ({ waffle }) => {
		const btn = waffle.page.locator('[data-testid="toolbar-btn-shell"]');
		await btn.click();
		await waffle.page.waitForTimeout(500);

		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});

test.describe('modeling buttons after extrude', () => {
	test('fillet/chamfer/shell buttons visible after creating extrusion', async ({ waffle }) => {
		// Create sketch + extrude
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {}

		// After extrude, modeling buttons should still be visible
		await expect(waffle.page.locator('[data-testid="toolbar-btn-fillet"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="toolbar-btn-chamfer"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="toolbar-btn-shell"]')).toBeVisible();
	});
});
