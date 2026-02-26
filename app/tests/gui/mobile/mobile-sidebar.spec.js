/**
 * Mobile sidebar tests — panel open/close, backdrop, viewport containment, scrollability.
 *
 * These tests require mobile layout (viewport <= 768px), skipped on landscape.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { isMobileViewport } from '../helpers/mobile.js';

test.describe('Mobile sidebar', () => {
	test.beforeEach(async ({ waffle }) => {
		const isMobile = await isMobileViewport(waffle.page);
		test.skip(!isMobile, 'Skipping mobile sidebar test on wide viewport');
	});

	test('opening left panel shows backdrop', async ({ waffle }) => {
		const page = waffle.page;
		const treeFab = page.locator('[data-testid="mobile-toggle-tree"]');

		// No backdrop initially
		await expect(page.locator('.mobile-backdrop')).toHaveCount(0);

		await treeFab.click();

		const backdrop = page.locator('.mobile-backdrop');
		await expect(backdrop).toBeVisible();
	});

	test('opening right panel after closing left panel', async ({ waffle }) => {
		const page = waffle.page;
		const treeFab = page.locator('[data-testid="mobile-toggle-tree"]');
		const propsFab = page.locator('[data-testid="mobile-toggle-props"]');
		const leftPanel = page.locator('.mobile-panel-left');
		const rightPanel = page.locator('.mobile-panel-right');

		// Open left panel
		await treeFab.click();
		await expect(leftPanel).toHaveClass(/\bopen\b/);
		await expect(rightPanel).not.toHaveClass(/\bopen\b/);

		// Close left panel via backdrop (backdrop intercepts pointer events over FABs)
		const backdrop = page.locator('.mobile-backdrop');
		await backdrop.click();
		await expect(leftPanel).not.toHaveClass(/\bopen\b/);

		// Now open right panel
		await propsFab.click();
		await expect(rightPanel).toHaveClass(/\bopen\b/);
	});

	test('sidebar panel within viewport bounds when open', async ({ waffle }) => {
		const page = waffle.page;
		const treeFab = page.locator('[data-testid="mobile-toggle-tree"]');

		await treeFab.click();
		const leftPanel = page.locator('.mobile-panel-left');
		await expect(leftPanel).toHaveClass(/\bopen\b/);

		// Wait for slide transition to complete (200ms ease + rendering buffer)
		await page.waitForTimeout(500);

		const box = await leftPanel.boundingBox();
		expect(box).not.toBeNull();
		// Panel should be at or near the left edge of the viewport
		expect(box.x).toBeGreaterThanOrEqual(-2);
		const viewport = page.viewportSize();
		expect(box.x + box.width).toBeLessThanOrEqual(viewport.width + 1);
	});

	test('panel content is scrollable if tall', async ({ waffle }) => {
		const page = waffle.page;
		const treeFab = page.locator('[data-testid="mobile-toggle-tree"]');

		await treeFab.click();
		const leftPanel = page.locator('.mobile-panel-left');
		await expect(leftPanel).toHaveClass(/\bopen\b/);

		// Check that overflow-y allows scrolling
		const overflowY = await leftPanel.evaluate((el) => getComputedStyle(el).overflowY);
		expect(['auto', 'scroll']).toContain(overflowY);
	});

	test('panel closes on backdrop click', async ({ waffle }) => {
		const page = waffle.page;
		const propsFab = page.locator('[data-testid="mobile-toggle-props"]');
		const rightPanel = page.locator('.mobile-panel-right');

		// Open right panel
		await propsFab.click();
		await expect(rightPanel).toHaveClass(/\bopen\b/);

		// Click backdrop to close
		const backdrop = page.locator('.mobile-backdrop');
		await expect(backdrop).toBeVisible();
		await backdrop.click();

		// Panel should be closed
		await expect(rightPanel).not.toHaveClass(/\bopen\b/);
		// Backdrop should be gone
		await expect(page.locator('.mobile-backdrop')).toHaveCount(0);
	});
});
