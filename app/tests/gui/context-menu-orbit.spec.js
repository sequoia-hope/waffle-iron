/**
 * Context menu vs orbit drag tests — verifies that right-click drag (orbit)
 * does NOT show the context menu, while stationary right-click does.
 *
 * Bug: Right-click orbits the camera but the browser contextmenu event still
 * fires, showing the viewport context menu even during orbit drags.
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds } from './helpers/canvas.js';

test.describe('context menu during orbit', () => {
	test('right-click drag (orbit) should NOT show context menu', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Perform a right-button drag (orbit) across the canvas
		const startX = bounds.centerX - 50;
		const startY = bounds.centerY;
		const endX = bounds.centerX + 50;
		const endY = bounds.centerY + 30;

		await page.mouse.move(startX, startY);
		await page.mouse.down({ button: 'right' });
		// Move in steps to simulate drag
		for (let i = 1; i <= 5; i++) {
			const t = i / 5;
			await page.mouse.move(
				startX + (endX - startX) * t,
				startY + (endY - startY) * t
			);
		}
		await page.mouse.up({ button: 'right' });
		await page.waitForTimeout(300);

		// Context menu should NOT be visible after orbit drag
		const ctxMenu = page.locator('[data-testid="ctx-menu"]');
		const visible = await ctxMenu.isVisible().catch(() => false);
		expect(visible).toBe(false);
	});

	test('stationary right-click SHOULD show context menu', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Stationary right-click (no drag)
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		// Context menu SHOULD be visible
		const ctxMenu = page.locator('[data-testid="ctx-menu"]');
		await expect(ctxMenu).toBeVisible({ timeout: 2000 });
	});

	test('small right-click movement (< threshold) still shows context menu', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Right-click with tiny movement (2px, below threshold)
		const x = bounds.centerX;
		const y = bounds.centerY;

		await page.mouse.move(x, y);
		await page.mouse.down({ button: 'right' });
		await page.mouse.move(x + 2, y + 1);
		await page.mouse.up({ button: 'right' });
		await page.waitForTimeout(300);

		// Context menu SHOULD still appear for small movements
		const ctxMenu = page.locator('[data-testid="ctx-menu"]');
		await expect(ctxMenu).toBeVisible({ timeout: 2000 });
	});
});
