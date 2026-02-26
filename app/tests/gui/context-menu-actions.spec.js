/**
 * Context menu action tests — verifies context menu items execute correctly,
 * context menu behavior with different tool states, and dismiss behavior.
 *
 * Complements viewcube-contextmenu.spec.js (which tests basic open/close/items)
 * and context-menu-orbit.spec.js (which tests drag vs click distinction).
 *
 * This file focuses on:
 * - Context menu actions execute and produce correct results
 * - Context menu behavior after geometry creation
 * - Right-click with active tools
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds } from './helpers/canvas.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	isSketchActive,
	getFeatureCount,
	waitForEntityCount,
	waitForFeatureCount,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Helper: open context menu at canvas center.
 */
async function openContextMenu(page) {
	const bounds = await getCanvasBounds(page);
	expect(bounds).not.toBeNull();
	await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
	await page.waitForTimeout(300);
	const menu = page.locator('[data-testid="ctx-menu"]');
	await expect(menu).toBeVisible({ timeout: 3000 });
	return menu;
}

test.describe('context menu view actions', () => {
	test('Top View action snaps camera to top-down', async ({ waffle }) => {
		const page = waffle.page;
		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		await openContextMenu(page);
		await page.locator('[data-testid="ctx-view-top"]').click();
		await page.waitForTimeout(500);

		// Menu should close
		await expect(page.locator('[data-testid="ctx-menu"]')).not.toBeVisible();

		// Camera Y component should dominate (top-down view)
		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();
		const absX = Math.abs(state.position[0]);
		const absY = Math.abs(state.position[1]);
		const absZ = Math.abs(state.position[2]);
		expect(absY).toBeGreaterThan(absX);
		expect(absY).toBeGreaterThan(absZ);
	});

	test('Right View action snaps camera', async ({ waffle }) => {
		const page = waffle.page;
		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		await openContextMenu(page);
		await page.locator('[data-testid="ctx-view-right"]').click();
		await page.waitForTimeout(500);

		await expect(page.locator('[data-testid="ctx-menu"]')).not.toBeVisible();

		// Right view: camera X-component should dominate
		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();
		const absX = Math.abs(state.position[0]);
		const absY = Math.abs(state.position[1]);
		const absZ = Math.abs(state.position[2]);
		expect(absX).toBeGreaterThan(absY);
		expect(absX).toBeGreaterThan(absZ);
	});

	test('Isometric action snaps camera to iso view', async ({ waffle }) => {
		const page = waffle.page;
		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		// First snap to front to establish non-iso position
		await page.locator('[data-testid="viewcube-btn-front"]').click();
		await page.waitForTimeout(300);

		// Now use context menu iso
		await openContextMenu(page);
		await page.locator('[data-testid="ctx-view-iso"]').click();
		await page.waitForTimeout(500);

		await expect(page.locator('[data-testid="ctx-menu"]')).not.toBeVisible();

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();

		// Iso: direction components should be roughly equal
		const dx = Math.abs(state.position[0] - state.target[0]);
		const dy = Math.abs(state.position[1] - state.target[1]);
		const dz = Math.abs(state.position[2] - state.target[2]);
		const max = Math.max(dx, dy, dz);
		const min = Math.min(dx, dy, dz);
		expect(min / max).toBeGreaterThan(0.5);
	});
});

test.describe('context menu New Sketch action', () => {
	test('New Sketch (XY) enters sketch mode from context menu', async ({ waffle }) => {
		const page = waffle.page;

		await openContextMenu(page);
		await page.locator('[data-testid="ctx-new-sketch"]').click();

		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		const active = await isSketchActive(page);
		expect(active).toBe(true);

		// Default sketch on XY plane (normal = [0,0,1])
		const state = await page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.normal).toEqual([0, 0, 1]);
	});

	test('context menu hidden after entering sketch via New Sketch', async ({ waffle }) => {
		const page = waffle.page;

		await openContextMenu(page);
		await page.locator('[data-testid="ctx-new-sketch"]').click();

		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		// Context menu should be hidden in sketch mode
		await expect(page.locator('[data-testid="ctx-menu"]')).not.toBeVisible();
	});
});

test.describe('context menu dismiss behavior', () => {
	test('second right-click repositions context menu', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// First right-click at center
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await page.waitForTimeout(300);
		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).toBeVisible({ timeout: 3000 });

		// Get initial menu position
		const box1 = await menu.boundingBox();

		// Dismiss by clicking away
		await page.mouse.click(bounds.centerX - 100, bounds.centerY - 100);
		await page.waitForTimeout(300);
		await expect(menu).not.toBeVisible();

		// Second right-click at a different position
		await page.mouse.click(bounds.centerX + 100, bounds.centerY + 50, { button: 'right' });
		await page.waitForTimeout(300);
		await expect(menu).toBeVisible({ timeout: 3000 });

		// Menu should appear at a different position
		const box2 = await menu.boundingBox();
		expect(box2).not.toBeNull();
		// Position should differ (at least one coordinate should be different)
		const positionChanged = box2.x !== box1.x || box2.y !== box1.y;
		expect(positionChanged).toBe(true);
	});

	test('Escape key does not open context menu', async ({ waffle }) => {
		const page = waffle.page;

		// Press Escape with no menu open — should not open context menu
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		const menu = page.locator('[data-testid="ctx-menu"]');
		await expect(menu).not.toBeVisible();
	});
});

test.describe('context menu with geometry', () => {
	test('context menu works after creating extruded geometry', async ({ waffle }) => {
		const page = waffle.page;

		// Create a sketch + extrude
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -50, -50, 50, 50);
		try { await waitForEntityCount(page, 8, 3000); } catch { /* draw might not produce 8 */ }
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		await page.evaluate(() => window.__waffle.showExtrudeDialog());
		await page.waitForTimeout(100);
		await page.evaluate(() => window.__waffle.applyExtrude(20, 0, false));
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);
		await page.waitForTimeout(300);

		// Context menu should still work
		const menu = await openContextMenu(page);
		await expect(menu).toBeVisible();

		// All standard items should be present
		await expect(page.locator('[data-testid="ctx-new-sketch"]')).toBeVisible();
		await expect(page.locator('[data-testid="ctx-fit-all"]')).toBeVisible();
		await expect(page.locator('[data-testid="ctx-view-front"]')).toBeVisible();
	});
});
