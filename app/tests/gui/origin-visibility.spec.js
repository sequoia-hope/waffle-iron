/**
 * Origin visibility tests — datum planes and axes visibility toggles
 * in the feature tree, including right-click context menu bulk actions.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch } from './helpers/toolbar.js';

test.describe('origin plane visibility', () => {
	test('plane visibility toggles hide/show in viewport', async ({ waffle }) => {
		const page = waffle.page;

		// All planes visible by default
		const frontVisible = await page.evaluate(() => window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000001'));
		expect(frontVisible).toBe(true);

		// Click the visibility toggle for the Front plane
		const toggleBtn = page.locator('[data-testid="plane-visibility-front"]');
		await expect(toggleBtn).toBeVisible();
		await toggleBtn.click();

		// Front plane should now be hidden
		const frontHidden = await page.evaluate(() => window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000001'));
		expect(frontHidden).toBe(false);

		// Click again to show
		await toggleBtn.click();
		const frontShown = await page.evaluate(() => window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000001'));
		expect(frontShown).toBe(true);
	});

	test('plane items dim when hidden', async ({ waffle }) => {
		const page = waffle.page;

		const planeItem = page.locator('[data-testid="origin-plane-front"]');
		await expect(planeItem).toBeVisible();

		// Initially should NOT have hidden-item class
		await expect(planeItem).not.toHaveClass(/hidden-item/);

		// Toggle visibility off
		const toggleBtn = page.locator('[data-testid="plane-visibility-front"]');
		await toggleBtn.click();

		// Should now have hidden-item class (dimmed)
		await expect(planeItem).toHaveClass(/hidden-item/);
	});

	test('right-click visible plane shows "Hide All Planes"', async ({ waffle }) => {
		const page = waffle.page;

		const planeItem = page.locator('[data-testid="origin-plane-front"]');
		await planeItem.click({ button: 'right' });

		const hideBtn = page.locator('[data-testid="ft-ctx-hide-all-planes"]');
		await expect(hideBtn).toBeVisible();

		// "Show All Planes" should NOT be visible (plane is visible)
		const showBtn = page.locator('[data-testid="ft-ctx-show-all-planes"]');
		await expect(showBtn).not.toBeVisible();
	});

	test('Hide All Planes hides all planes via context menu', async ({ waffle }) => {
		const page = waffle.page;

		// Right-click on Front plane (visible)
		const planeItem = page.locator('[data-testid="origin-plane-front"]');
		await planeItem.click({ button: 'right' });

		// Click "Hide All Planes"
		const hideBtn = page.locator('[data-testid="ft-ctx-hide-all-planes"]');
		await hideBtn.click();

		// All planes should be hidden
		const result = await page.evaluate(() => ({
			front: window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000001'),
			top: window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000002'),
			right: window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000003'),
		}));
		expect(result.front).toBe(false);
		expect(result.top).toBe(false);
		expect(result.right).toBe(false);
	});

	test('right-click hidden plane shows "Show All Planes"', async ({ waffle }) => {
		const page = waffle.page;

		// First hide the front plane
		await page.evaluate(() => window.__waffle.togglePlaneVisibility('00000000-0000-0000-0000-000000000001'));
		await page.waitForTimeout(100);

		// Right-click on hidden Front plane
		const planeItem = page.locator('[data-testid="origin-plane-front"]');
		await planeItem.click({ button: 'right' });

		const showBtn = page.locator('[data-testid="ft-ctx-show-all-planes"]');
		await expect(showBtn).toBeVisible();

		// "Hide All Planes" should NOT be visible (plane is hidden)
		const hideBtn = page.locator('[data-testid="ft-ctx-hide-all-planes"]');
		await expect(hideBtn).not.toBeVisible();
	});

	test('Show All Planes shows all planes via context menu', async ({ waffle }) => {
		const page = waffle.page;

		// Hide all planes first
		await page.evaluate(() => {
			window.__waffle.togglePlaneVisibility('00000000-0000-0000-0000-000000000001');
			window.__waffle.togglePlaneVisibility('00000000-0000-0000-0000-000000000002');
			window.__waffle.togglePlaneVisibility('00000000-0000-0000-0000-000000000003');
		});
		await page.waitForTimeout(100);

		// Right-click on hidden Front plane
		const planeItem = page.locator('[data-testid="origin-plane-front"]');
		await planeItem.click({ button: 'right' });

		// Click "Show All Planes"
		const showBtn = page.locator('[data-testid="ft-ctx-show-all-planes"]');
		await showBtn.click();

		// All planes should be visible
		const result = await page.evaluate(() => ({
			front: window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000001'),
			top: window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000002'),
			right: window.__waffle.isPlaneVisible('00000000-0000-0000-0000-000000000003'),
		}));
		expect(result.front).toBe(true);
		expect(result.top).toBe(true);
		expect(result.right).toBe(true);
	});
});

test.describe('origin axis visibility', () => {
	test('axis items are displayed in origin section', async ({ waffle }) => {
		const page = waffle.page;

		await expect(page.locator('[data-testid="origin-axis-x"]')).toBeVisible();
		await expect(page.locator('[data-testid="origin-axis-y"]')).toBeVisible();
		await expect(page.locator('[data-testid="origin-axis-z"]')).toBeVisible();
	});

	test('axis visibility toggles hide/show', async ({ waffle }) => {
		const page = waffle.page;

		// X axis visible by default
		const xVisible = await page.evaluate(() => window.__waffle.isAxisVisible('x'));
		expect(xVisible).toBe(true);

		// Click the visibility toggle for X axis
		const toggleBtn = page.locator('[data-testid="axis-visibility-x"]');
		await toggleBtn.click();

		const xHidden = await page.evaluate(() => window.__waffle.isAxisVisible('x'));
		expect(xHidden).toBe(false);

		// Click again to show
		await toggleBtn.click();
		const xShown = await page.evaluate(() => window.__waffle.isAxisVisible('x'));
		expect(xShown).toBe(true);
	});

	test('right-click visible axis shows "Hide All Axes"', async ({ waffle }) => {
		const page = waffle.page;

		const axisItem = page.locator('[data-testid="origin-axis-x"]');
		await axisItem.click({ button: 'right' });

		const hideBtn = page.locator('[data-testid="ft-ctx-hide-all-axes"]');
		await expect(hideBtn).toBeVisible();
	});

	test('Hide All Axes hides all axes via context menu', async ({ waffle }) => {
		const page = waffle.page;

		const axisItem = page.locator('[data-testid="origin-axis-y"]');
		await axisItem.click({ button: 'right' });

		const hideBtn = page.locator('[data-testid="ft-ctx-hide-all-axes"]');
		await hideBtn.click();

		const result = await page.evaluate(() => ({
			x: window.__waffle.isAxisVisible('x'),
			y: window.__waffle.isAxisVisible('y'),
			z: window.__waffle.isAxisVisible('z'),
		}));
		expect(result.x).toBe(false);
		expect(result.y).toBe(false);
		expect(result.z).toBe(false);
	});

	test('Show All Axes shows all axes via context menu', async ({ waffle }) => {
		const page = waffle.page;

		// Hide all axes first
		await page.evaluate(() => {
			window.__waffle.toggleAxisVisibility('x');
			window.__waffle.toggleAxisVisibility('y');
			window.__waffle.toggleAxisVisibility('z');
		});
		await page.waitForTimeout(100);

		const axisItem = page.locator('[data-testid="origin-axis-z"]');
		await axisItem.click({ button: 'right' });

		const showBtn = page.locator('[data-testid="ft-ctx-show-all-axes"]');
		await showBtn.click();

		const result = await page.evaluate(() => ({
			x: window.__waffle.isAxisVisible('x'),
			y: window.__waffle.isAxisVisible('y'),
			z: window.__waffle.isAxisVisible('z'),
		}));
		expect(result.x).toBe(true);
		expect(result.y).toBe(true);
		expect(result.z).toBe(true);
	});
});

test.describe('sketch visibility context menu', () => {
	test('right-click visible sketch shows "Hide All Sketches"', async ({ waffle }) => {
		const page = waffle.page;

		// Create a sketch to have something to right-click
		await clickSketch(page, 'front');
		await clickFinishSketch(page);

		// Right-click the sketch in the feature tree
		const featureItem = page.locator('[data-testid="feature-item-0"]');
		await featureItem.click({ button: 'right' });

		// Should show "Hide All Sketches" (sketch is visible by default)
		const hideBtn = page.locator('[data-testid="ft-ctx-hide-all-sketches"]');
		await expect(hideBtn).toBeVisible();
	});

	test('Hide All Sketches hides all sketches', async ({ waffle }) => {
		const page = waffle.page;

		// Create two sketches
		await clickSketch(page, 'front');
		await clickFinishSketch(page);

		await clickSketch(page, 'top');
		await clickFinishSketch(page);

		// Right-click first sketch
		const featureItem = page.locator('[data-testid="feature-item-0"]');
		await featureItem.click({ button: 'right' });

		// Click "Hide All Sketches"
		const hideBtn = page.locator('[data-testid="ft-ctx-hide-all-sketches"]');
		await hideBtn.click();

		// Both sketches should be hidden
		const result = await page.evaluate(() => {
			const tree = window.__waffle.getFeatureTree();
			return tree.features
				.filter(f => f.operation?.type === 'Sketch')
				.map(f => window.__waffle.isSketchVisible(f.id));
		});
		expect(result).toEqual([false, false]);
	});
});
