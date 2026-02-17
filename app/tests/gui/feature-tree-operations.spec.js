/**
 * Feature tree operations tests — rollback slider interaction,
 * suppress/unsuppress via context menu, and delete cascading.
 *
 * Focuses on operations that affect the model state.
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
	getFeatureCount,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

// Selector for feature items only (excludes origin plane items)
const FEATURE_ITEM = '.tree-item:not(.origin-item)';

/**
 * Helper: create a finished sketch with a rectangle (1 feature).
 */
async function createSketchFeature(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('fto-sketch-draw-failed');
	}

	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('fto-sketch-finish-failed');
	}
}

/**
 * Helper: create a sketch + extrude (2 features).
 */
async function createSketchAndExtrude(waffle) {
	await createSketchFeature(waffle);

	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try {
		await waitForFeatureCount(waffle.page, 2, 10000);
	} catch {
		await waffle.dumpState('fto-extrude-failed');
	}
}

test.describe('rollback slider interaction', () => {
	test('rollback slider dims features after rollback point', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Slider active_index is 0-based: value 0 means "last active feature is index 0"
		// So feature[1] (extrude) gets dimmed (i > 0 = true)
		await waffle.page.evaluate(() => {
			const slider = document.querySelector('.rollback-slider');
			slider.value = '0';
			slider.dispatchEvent(new Event('input', { bubbles: true }));
		});
		await waffle.page.waitForTimeout(500);

		// The second tree item (extrude) should have .after-rollback class
		const secondItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await expect(secondItem).toHaveClass(/after-rollback/);

		// The first item (sketch) should NOT have .after-rollback
		const firstItem = waffle.page.locator(FEATURE_ITEM).nth(0);
		const firstClasses = await firstItem.getAttribute('class');
		expect(firstClasses).not.toContain('after-rollback');
	});

	test('rollback slider at 1 with 2 features shows all active', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Slider value 1 means active_index=1: features 0..1 all active (nothing dimmed)
		await waffle.page.evaluate(() => {
			const slider = document.querySelector('.rollback-slider');
			slider.value = '1';
			slider.dispatchEvent(new Event('input', { bubbles: true }));
		});
		await waffle.page.waitForTimeout(500);

		// Neither feature should be dimmed (both have index <= 1)
		const items = waffle.page.locator(FEATURE_ITEM);
		const count = await items.count();
		expect(count).toBe(2);

		for (let i = 0; i < count; i++) {
			const classes = await items.nth(i).getAttribute('class');
			expect(classes).not.toContain('after-rollback');
		}
	});

	test('rollback back to max restores all features', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// First rollback to 0 (only feature[0] active, feature[1] dimmed)
		await waffle.page.evaluate(() => {
			const slider = document.querySelector('.rollback-slider');
			slider.value = '0';
			slider.dispatchEvent(new Event('input', { bubbles: true }));
		});
		await waffle.page.waitForTimeout(500);

		// Verify rollback took effect
		const secondItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await expect(secondItem).toHaveClass(/after-rollback/);

		// Now set slider back to max (2 = features.length → null → no rollback)
		await waffle.page.evaluate(() => {
			const slider = document.querySelector('.rollback-slider');
			slider.value = '2';
			slider.dispatchEvent(new Event('input', { bubbles: true }));
		});
		await waffle.page.waitForTimeout(500);

		// No items should have .after-rollback class
		const items = waffle.page.locator(FEATURE_ITEM);
		const count = await items.count();
		expect(count).toBe(2);

		for (let i = 0; i < count; i++) {
			const classes = await items.nth(i).getAttribute('class');
			expect(classes).not.toContain('after-rollback');
		}
	});
});

test.describe('suppress and unsuppress via context menu', () => {
	test('suppress feature via context menu adds suppressed class', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const firstItem = waffle.page.locator(FEATURE_ITEM).nth(0);

		// Right-click to open context menu
		await firstItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		// Click Suppress
		await waffle.page.locator('.context-menu .ctx-item >> text=Suppress').click();
		await waffle.page.waitForTimeout(500);

		// First item should have .suppressed class
		await expect(firstItem).toHaveClass(/suppressed/);
	});

	test('unsuppress restores feature', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const firstItem = waffle.page.locator(FEATURE_ITEM).nth(0);

		// Suppress first
		await firstItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);
		await waffle.page.locator('.context-menu .ctx-item >> text=Suppress').click();
		await waffle.page.waitForTimeout(500);

		// Verify suppressed
		await expect(firstItem).toHaveClass(/suppressed/);

		// Right-click again and unsuppress
		await firstItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);
		await waffle.page.locator('.context-menu .ctx-item >> text=Unsuppress').click();
		await waffle.page.waitForTimeout(500);

		// .suppressed class should be removed
		const classes = await firstItem.getAttribute('class');
		expect(classes).not.toContain('suppressed');
	});
});

test.describe('delete operations', () => {
	test('delete extrude via context menu removes it', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Verify we start with 2 features
		const itemsBefore = await waffle.page.locator(FEATURE_ITEM).count();
		expect(itemsBefore).toBe(2);

		// Right-click second item (extrude) and delete
		const secondItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await secondItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		await waffle.page.locator('.context-menu .ctx-item.danger >> text=Delete').click();
		await waffle.page.waitForTimeout(500);

		// Feature count should drop to 1
		const itemsAfter = await waffle.page.locator(FEATURE_ITEM).count();
		expect(itemsAfter).toBe(1);
	});

	test('delete sketch removes it from tree', async ({ waffle }) => {
		await createSketchFeature(waffle);

		// Verify we start with 1 feature
		const itemsBefore = await waffle.page.locator(FEATURE_ITEM).count();
		expect(itemsBefore).toBe(1);

		// Right-click and delete
		const firstItem = waffle.page.locator(FEATURE_ITEM).nth(0);
		await firstItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		await waffle.page.locator('.context-menu .ctx-item.danger >> text=Delete').click();
		await waffle.page.waitForTimeout(500);

		// Feature count should be 0
		const itemsAfter = await waffle.page.locator(FEATURE_ITEM).count();
		expect(itemsAfter).toBe(0);

		// Empty state text should be visible again
		const emptyState = waffle.page.locator('.feature-tree .empty-state');
		await expect(emptyState).toBeVisible();
	});
});
