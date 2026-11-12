/**
 * Mobile sidebar tests — panel open/close, backdrop, viewport containment, scrollability.
 *
 * These tests require mobile layout (viewport <= 768px), skipped on landscape.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { isMobileViewport } from '../helpers/mobile.js';

/**
 * Tap the backdrop where a user can reach it: on the strip the open panel
 * leaves uncovered.
 *
 * The backdrop fills the window and the panel sits on top of it, 260 px of a
 * 440 px portrait window (`.mobile-panel { width: min(260px, 75vw) }`), so
 * the backdrop's CENTER — where a bare `backdrop.click()` aims — is under the
 * panel. Such a click can only land while the panel is still sliding in
 * (0.2 s) and short of the middle; once the slide has finished it waits on a
 * point that is covered by design until the test times out.
 *
 * So: let the slide come to rest, then aim at the middle of what is left.
 * Playwright still hit-tests that point, so a panel that grew to cover the
 * whole backdrop fails here rather than passing.
 *
 * @param {import('@playwright/test').Page} page
 * @param {import('@playwright/test').Locator} panel - the open `.mobile-panel`
 */
async function tapBackdropBeside(page, panel) {
	const backdrop = page.locator('.mobile-backdrop');
	await expect(backdrop).toBeVisible();
	// `.open` is `transform: translateX(0)`; mid-slide it is a translation.
	await expect(panel).toHaveCSS('transform', 'matrix(1, 0, 0, 1, 0, 0)');

	const view = page.viewportSize();
	const box = await panel.boundingBox();
	expect(box).not.toBeNull();
	// A left panel leaves the strip to its right, a right panel the one to
	// its left.
	const [from, to] = box.x <= 0 ? [box.x + box.width, view.width] : [0, box.x];
	expect(to - from, 'backdrop strip left uncovered by the open panel').toBeGreaterThan(0);
	await backdrop.click({ position: { x: (from + to) / 2, y: view.height / 2 } });
}

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
		await tapBackdropBeside(page, leftPanel);
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
		await tapBackdropBeside(page, rightPanel);

		// Panel should be closed
		await expect(rightPanel).not.toHaveClass(/\bopen\b/);
		// Backdrop should be gone
		await expect(page.locator('.mobile-backdrop')).toHaveCount(0);
	});
});
