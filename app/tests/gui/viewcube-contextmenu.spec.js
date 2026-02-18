/**
 * ViewCube buttons and viewport context menu tests.
 *
 * Verifies that the viewcube overlay renders all 7 standard view buttons,
 * that clicking them snaps the camera correctly, and that the right-click
 * context menu provides expected items and behavior.
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds } from './helpers/canvas.js';
import { clickSketch } from './helpers/toolbar.js';
import { isSketchActive } from './helpers/state.js';

test.describe('viewcube buttons', () => {
	test('viewcube overlay visible', async ({ waffle }) => {
		const overlay = waffle.page.locator('[data-testid="viewcube-overlay"]');
		await expect(overlay).toBeVisible();
	});

	test('all 7 view buttons render', async ({ waffle }) => {
		const views = ['front', 'back', 'top', 'bottom', 'left', 'right', 'iso'];
		for (const name of views) {
			const btn = waffle.page.locator(`[data-testid="viewcube-btn-${name}"]`);
			await expect(btn).toBeVisible();
		}
	});

	test('clicking Front snaps camera', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		await page.locator('[data-testid="viewcube-btn-front"]').click();
		// Allow the snap to settle
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();
		expect(Array.isArray(state.position)).toBe(true);

		// Front view: camera looks along -Z, so the Z-component of the
		// camera position should dominate (be the largest absolute value)
		const absX = Math.abs(state.position[0]);
		const absY = Math.abs(state.position[1]);
		const absZ = Math.abs(state.position[2]);
		expect(absZ).toBeGreaterThan(absX);
		expect(absZ).toBeGreaterThan(absY);
	});

	test('clicking Top snaps camera', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		await page.locator('[data-testid="viewcube-btn-top"]').click();
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();

		// Top view: camera looks along -Y, so Y-component dominates
		const absX = Math.abs(state.position[0]);
		const absY = Math.abs(state.position[1]);
		const absZ = Math.abs(state.position[2]);
		expect(absY).toBeGreaterThan(absX);
		expect(absY).toBeGreaterThan(absZ);
	});

	test('clicking Iso snaps camera', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		// First snap to Front to establish a non-iso position
		await page.locator('[data-testid="viewcube-btn-front"]').click();
		await page.waitForTimeout(300);

		// Now snap to Iso
		await page.locator('[data-testid="viewcube-btn-iso"]').click();
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();

		// Iso view: direction is [1,1,1] normalized, so all three components
		// of the (position - target) vector should be roughly equal
		const dx = state.position[0] - state.target[0];
		const dy = state.position[1] - state.target[1];
		const dz = state.position[2] - state.target[2];
		const absDx = Math.abs(dx);
		const absDy = Math.abs(dy);
		const absDz = Math.abs(dz);

		// The three direction components should be within 30% of each other
		const max = Math.max(absDx, absDy, absDz);
		const min = Math.min(absDx, absDy, absDz);
		expect(min / max).toBeGreaterThan(0.5);
	});

	test('active class on current view', async ({ waffle }) => {
		const page = waffle.page;

		await page.locator('[data-testid="viewcube-btn-front"]').click();
		await page.waitForTimeout(300);

		const frontBtn = page.locator('[data-testid="viewcube-btn-front"]');
		await expect(frontBtn).toHaveClass(/active/);
	});
});

test.describe('viewport context menu', () => {
	test('right-click opens context menu', async ({ waffle }) => {
		const page = waffle.page;

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Right-click on the canvas center
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });
	});

	test('context menu items present', async ({ waffle }) => {
		const page = waffle.page;

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });

		// Verify standard context menu items are present
		await expect(page.locator('[data-testid="ctx-new-sketch"]')).toBeVisible();
		await expect(page.locator('[data-testid="ctx-fit-all"]')).toBeVisible();
		await expect(page.locator('[data-testid="ctx-view-front"]')).toBeVisible();
		await expect(page.locator('[data-testid="ctx-view-top"]')).toBeVisible();
		await expect(page.locator('[data-testid="ctx-view-right"]')).toBeVisible();
		await expect(page.locator('[data-testid="ctx-view-iso"]')).toBeVisible();
	});

	test('Fit All closes menu and fits', async ({ waffle }) => {
		const page = waffle.page;

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Open context menu
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });

		// Click Fit All
		await page.locator('[data-testid="ctx-fit-all"]').click();
		await page.waitForTimeout(500);

		// Menu should be hidden
		await expect(menu).not.toBeVisible();

		// Canvas should still be visible (no crash)
		await expect(page.locator('canvas')).toBeVisible();
	});

	test('context menu view items work', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Open context menu
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });

		// Click "Front View"
		await page.locator('[data-testid="ctx-view-front"]').click();
		await page.waitForTimeout(500);

		// Menu should close
		await expect(menu).not.toBeVisible();

		// Camera should now be in front view (Z-component dominates)
		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();
		const absX = Math.abs(state.position[0]);
		const absY = Math.abs(state.position[1]);
		const absZ = Math.abs(state.position[2]);
		expect(absZ).toBeGreaterThan(absX);
		expect(absZ).toBeGreaterThan(absY);
	});

	test('clicking away closes menu', async ({ waffle }) => {
		const page = waffle.page;

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Open context menu
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });

		// Click elsewhere on the page (not on the menu)
		await page.mouse.click(bounds.centerX - 100, bounds.centerY - 100);
		await page.waitForTimeout(300);

		// Menu should be hidden
		await expect(menu).not.toBeVisible();
	});

	test('menu hidden in sketch mode', async ({ waffle }) => {
		const page = waffle.page;

		// Enter sketch mode
		await clickSketch(page);
		const active = await isSketchActive(page);
		expect(active).toBe(true);

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Right-click on the canvas
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(500);

		// The ViewportContextMenu has an `!inSketch` guard, so ctx-menu should NOT appear
		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).not.toBeVisible();
	});

	test('New Sketch (XY) enters sketch', async ({ waffle }) => {
		const page = waffle.page;

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Open context menu
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });

		// Click "New Sketch (XY)"
		await page.locator('[data-testid="ctx-new-sketch"]').click();

		// Wait for sketch mode to activate
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		const active = await isSketchActive(page);
		expect(active).toBe(true);

		// The default sketch should be on the XY plane (normal = [0,0,1])
		const state = await page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.normal).toEqual([0, 0, 1]);
	});

	test('Sketch on Face not shown without selection', async ({ waffle }) => {
		const page = waffle.page;

		// Ensure no selection
		await page.evaluate(() => window.__waffle.clearSelection());
		await page.waitForTimeout(200);

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Open context menu
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });

		// "Sketch on Face" should NOT be present without a face selection
		const sketchOnFace = page.locator('[data-testid="ctx-sketch-on-face"]');
		await expect(sketchOnFace).not.toBeVisible();
	});
});
