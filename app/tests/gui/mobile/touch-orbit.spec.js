/**
 * Touch orbit tests — verifies single-finger touch drag orbits the camera
 * and does NOT trigger the viewport context menu.
 *
 * Runs in mobile-portrait project (hasTouch: true).
 */
import { test, expect } from '../helpers/waffle-test.js';
import { getCanvasBounds, touchDrag, longPressTouch } from '../helpers/canvas.js';

test.describe('touch orbit', () => {
	test('single-finger touch drag orbits camera', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Capture camera position before drag
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		expect(before).not.toBeNull();

		// Perform a significant touch drag across the canvas
		const startX = bounds.centerX - 60;
		const startY = bounds.centerY;
		const endX = bounds.centerX + 60;
		const endY = bounds.centerY + 40;
		await touchDrag(page, startX, startY, endX, endY, 8);

		// Camera position should have changed (orbit worked)
		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();

		// Camera starts at ~0.052 units from origin, so even moderate orbit
		// produces position changes of ~0.01-0.04 units.
		const dist = Math.sqrt(
			(after.position[0] - before.position[0]) ** 2 +
			(after.position[1] - before.position[1]) ** 2 +
			(after.position[2] - before.position[2]) ** 2
		);
		expect(dist).toBeGreaterThan(0.005);

		// Context menu should NOT be visible after touch drag
		const ctxMenu = page.locator('[data-testid="ctx-menu"]');
		const visible = await ctxMenu.isVisible().catch(() => false);
		expect(visible).toBe(false);
	});

	test('long touch drag does not trigger context menu', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Perform a slow touch drag (each step has a delay, total >500ms)
		// to exceed the long-press duration threshold
		const startX = bounds.centerX - 40;
		const startY = bounds.centerY;
		const endX = bounds.centerX + 40;
		const endY = bounds.centerY + 30;

		// Dispatch touch events with delays between moves to exceed long-press timer
		await page.evaluate(({ sx, sy }) => {
			const canvas = document.querySelector('canvas');
			canvas.dispatchEvent(new PointerEvent('pointerdown', {
				bubbles: true, cancelable: true,
				clientX: sx, clientY: sy,
				pointerId: 1, pointerType: 'touch', isPrimary: true, pressure: 0.5,
			}));
		}, { sx: startX, sy: startY });

		// Move slowly over 600ms (exceeds 500ms long-press threshold)
		const steps = 12;
		for (let i = 1; i <= steps; i++) {
			const t = i / steps;
			const x = startX + (endX - startX) * t;
			const y = startY + (endY - startY) * t;
			await page.evaluate(({ cx, cy }) => {
				const canvas = document.querySelector('canvas');
				canvas.dispatchEvent(new PointerEvent('pointermove', {
					bubbles: true, cancelable: true,
					clientX: cx, clientY: cy,
					pointerId: 1, pointerType: 'touch', isPrimary: true, pressure: 0.5,
				}));
			}, { cx: x, cy: y });
			await page.waitForTimeout(50);
		}

		await page.evaluate(({ ex, ey }) => {
			const canvas = document.querySelector('canvas');
			canvas.dispatchEvent(new PointerEvent('pointerup', {
				bubbles: true, cancelable: true,
				clientX: ex, clientY: ey,
				pointerId: 1, pointerType: 'touch', isPrimary: true, pressure: 0,
			}));
		}, { ex: endX, ey: endY });
		await page.waitForTimeout(300);

		// Context menu must NOT appear — drag should have cancelled long-press
		const ctxMenu = page.locator('[data-testid="ctx-menu"]');
		const visible = await ctxMenu.isVisible().catch(() => false);
		expect(visible).toBe(false);
	});

	test('orbit works after long-press context menu', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Record initial camera state
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		expect(before).not.toBeNull();

		// Long-press to trigger context menu
		await longPressTouch(page, bounds.centerX, bounds.centerY);

		// Context menu should be visible
		const ctxMenu = page.locator('[data-testid="ctx-menu"]');
		await expect(ctxMenu).toBeVisible({ timeout: 2000 });

		// Touch drag to orbit — this also dismisses the context menu
		const startX = bounds.centerX - 60;
		const startY = bounds.centerY;
		const endX = bounds.centerX + 60;
		const endY = bounds.centerY + 40;
		await touchDrag(page, startX, startY, endX, endY, 8);

		// Camera position should have changed (orbit still works after long-press)
		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();

		const dist = Math.sqrt(
			(after.position[0] - before.position[0]) ** 2 +
			(after.position[1] - before.position[1]) ** 2 +
			(after.position[2] - before.position[2]) ** 2
		);
		expect(dist).toBeGreaterThan(0.005);

		// Context menu should be dismissed after touch interaction
		const menuVisible = await ctxMenu.isVisible().catch(() => false);
		expect(menuVisible).toBe(false);
	});
});
