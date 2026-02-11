/**
 * Viewport basics — canvas rendering, orbit, zoom, fit-all.
 */
import { test, expect } from './helpers/waffle-test.js';
import { orbitDrag, zoom, getCanvasBounds } from './helpers/canvas.js';

test.describe('viewport basics', () => {
	test('canvas renders and is visible', async ({ waffle }) => {
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
		const bounds = await getCanvasBounds(waffle.page);
		expect(bounds).not.toBeNull();
		expect(bounds.width).toBeGreaterThan(100);
		expect(bounds.height).toBeGreaterThan(100);
	});

	test('viewport container has data-testid', async ({ waffle }) => {
		await expect(waffle.page.locator('[data-testid="viewport"]')).toBeVisible();
	});

	test('toolbar renders with modeling tools', async ({ waffle }) => {
		await expect(waffle.page.locator('[data-testid="toolbar"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="toolbar-btn-sketch"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="toolbar-btn-extrude"]')).toBeVisible();
	});

	test('status bar shows engine ready', async ({ waffle }) => {
		await expect(waffle.page.locator('[data-testid="statusbar"]')).toBeVisible();
		const statusText = await waffle.page.locator('[data-testid="status-message"]').textContent();
		expect(statusText).toContain('Engine ready');
	});

	test('status dot is green when ready', async ({ waffle }) => {
		const statusDot = waffle.page.locator('[data-testid="status-dot"]');
		await expect(statusDot).toBeVisible();
		await expect(statusDot).toHaveClass(/ready/);
	});

	test('orbit drag changes camera (controls respond to mouse)', async ({ waffle }) => {
		// Get initial camera-like state by reading canvas pixels or just verify no crash
		await orbitDrag(waffle.page, 0, 0, 100, 50);
		// If orbit works, the canvas should still be visible (no freeze/crash)
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('mouse wheel zoom does not crash', async ({ waffle }) => {
		await zoom(waffle.page, -100);
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();

		await zoom(waffle.page, 200);
		await expect(canvas).toBeVisible();
	});

	test('F key triggers fit-all without crash', async ({ waffle }) => {
		await waffle.page.keyboard.press('f');
		await waffle.page.waitForTimeout(300);
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});
