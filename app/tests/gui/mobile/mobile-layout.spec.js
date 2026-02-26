/**
 * Mobile layout tests — viewport, toolbar, statusbar, sidebar toggles, canvas.
 *
 * Runs in mobile-portrait and mobile-landscape Playwright projects.
 * At 956px landscape width, the app uses desktop layout (breakpoint is 768px),
 * so mobile-specific tests are skipped in landscape via isMobileViewport().
 */
import { test, expect } from '../helpers/waffle-test.js';
import { assertNoOverflow, assertElementWithinBounds, isMobileViewport } from '../helpers/mobile.js';

test.describe('Layout — all viewports', () => {
	test('viewport fills available screen width', async ({ waffle }) => {
		const page = waffle.page;
		const appShell = page.locator('.app-shell');
		await expect(appShell).toBeVisible();

		const viewport = page.viewportSize();
		const box = await appShell.boundingBox();
		expect(box).not.toBeNull();
		expect(box.width).toBeGreaterThanOrEqual(viewport.width - 1);
	});

	test('no horizontal scrollbar', async ({ waffle }) => {
		const page = waffle.page;
		const hasHScroll = await page.evaluate(
			() => document.documentElement.scrollWidth > window.innerWidth
		);
		expect(hasHScroll).toBe(false);
	});

	test('no vertical scrollbar', async ({ waffle }) => {
		const page = waffle.page;
		const hasVScroll = await page.evaluate(
			() => document.documentElement.scrollHeight > window.innerHeight
		);
		expect(hasVScroll).toBe(false);
	});

	test('toolbar visible and within bounds', async ({ waffle }) => {
		const page = waffle.page;
		const toolbar = page.locator('[data-testid="toolbar"]');
		await expect(toolbar).toBeVisible();
		await assertElementWithinBounds(page, '[data-testid="toolbar"]', expect);
	});

	test('statusbar visible at bottom', async ({ waffle }) => {
		const page = waffle.page;
		const statusbar = page.locator('[data-testid="statusbar"]');
		await expect(statusbar).toBeVisible();

		const box = await statusbar.boundingBox();
		const viewport = page.viewportSize();
		// Statusbar should be near the bottom of the viewport
		expect(box.y + box.height).toBeGreaterThanOrEqual(viewport.height - 2);
	});

	test('canvas exists with non-zero size', async ({ waffle }) => {
		const page = waffle.page;
		const canvas = page.locator('canvas').first();
		await expect(canvas).toBeVisible();

		const box = await canvas.boundingBox();
		expect(box).not.toBeNull();
		expect(box.width).toBeGreaterThan(50);
		expect(box.height).toBeGreaterThan(50);
	});

	test('all top-level visible elements fit within window', async ({ waffle }) => {
		const page = waffle.page;
		await assertNoOverflow(
			page,
			['[data-testid="toolbar"]', '[data-testid="statusbar"]'],
			expect
		);
	});
});

test.describe('Mobile layout — portrait only', () => {
	test.beforeEach(async ({ waffle }) => {
		const isMobile = await isMobileViewport(waffle.page);
		test.skip(!isMobile, 'Skipping mobile-specific test on wide viewport');
	});

	test('app uses mobile layout class', async ({ waffle }) => {
		const page = waffle.page;
		const appShell = page.locator('.app-shell.mobile');
		await expect(appShell).toBeVisible();
	});

	test('sidebars hidden by default', async ({ waffle }) => {
		const page = waffle.page;
		const leftPanel = page.locator('.mobile-panel-left');
		const rightPanel = page.locator('.mobile-panel-right');

		// Panels exist but should NOT have the .open class
		await expect(leftPanel).not.toHaveClass(/\bopen\b/);
		await expect(rightPanel).not.toHaveClass(/\bopen\b/);
	});

	test('FAB toggle buttons visible', async ({ waffle }) => {
		const page = waffle.page;
		const treeFab = page.locator('[data-testid="mobile-toggle-tree"]');
		const propsFab = page.locator('[data-testid="mobile-toggle-props"]');

		await expect(treeFab).toBeVisible();
		await expect(propsFab).toBeVisible();
	});

	test('left FAB opens feature tree panel', async ({ waffle }) => {
		const page = waffle.page;
		const treeFab = page.locator('[data-testid="mobile-toggle-tree"]');
		const leftPanel = page.locator('.mobile-panel-left');

		await expect(leftPanel).not.toHaveClass(/\bopen\b/);
		await treeFab.click();
		await expect(leftPanel).toHaveClass(/\bopen\b/);

		// Backdrop should appear
		const backdrop = page.locator('.mobile-backdrop');
		await expect(backdrop).toBeVisible();
	});

	test('right FAB opens property editor panel', async ({ waffle }) => {
		const page = waffle.page;
		const propsFab = page.locator('[data-testid="mobile-toggle-props"]');
		const rightPanel = page.locator('.mobile-panel-right');

		await expect(rightPanel).not.toHaveClass(/\bopen\b/);
		await propsFab.click();
		await expect(rightPanel).toHaveClass(/\bopen\b/);

		// Backdrop should appear
		const backdrop = page.locator('.mobile-backdrop');
		await expect(backdrop).toBeVisible();
	});

	test('backdrop click closes panel', async ({ waffle }) => {
		const page = waffle.page;
		const treeFab = page.locator('[data-testid="mobile-toggle-tree"]');
		const leftPanel = page.locator('.mobile-panel-left');

		// Open the left panel
		await treeFab.click();
		await expect(leftPanel).toHaveClass(/\bopen\b/);

		// Click the backdrop
		const backdrop = page.locator('.mobile-backdrop');
		await expect(backdrop).toBeVisible();
		await backdrop.click();

		// Panel should be closed
		await expect(leftPanel).not.toHaveClass(/\bopen\b/);
	});
});
