/**
 * Feature tree origin section tests — verifies the origin expand/collapse toggle,
 * datum plane item display, selection behavior, and interaction with the __waffle API.
 */
import { test, expect } from './helpers/waffle-test.js';

test.describe('origin section visibility', () => {
	test('origin section toggle is visible', async ({ waffle }) => {
		const toggle = waffle.page.locator('[data-testid="origin-toggle"]');
		await expect(toggle).toBeVisible();
	});

	test('origin expanded by default with all three planes visible', async ({ waffle }) => {
		const front = waffle.page.locator('[data-testid="origin-plane-front"]');
		const top = waffle.page.locator('[data-testid="origin-plane-top"]');
		const right = waffle.page.locator('[data-testid="origin-plane-right"]');

		await expect(front).toBeVisible();
		await expect(top).toBeVisible();
		await expect(right).toBeVisible();
	});

	test('toggle collapses origin section', async ({ waffle }) => {
		const toggle = waffle.page.locator('[data-testid="origin-toggle"]');
		await toggle.click();
		await waffle.page.waitForTimeout(200);

		// Plane items should be hidden
		await expect(waffle.page.locator('[data-testid="origin-plane-front"]')).not.toBeVisible();
		await expect(waffle.page.locator('[data-testid="origin-plane-top"]')).not.toBeVisible();
		await expect(waffle.page.locator('[data-testid="origin-plane-right"]')).not.toBeVisible();
	});

	test('toggle re-expands origin section', async ({ waffle }) => {
		const toggle = waffle.page.locator('[data-testid="origin-toggle"]');

		// Collapse
		await toggle.click();
		await waffle.page.waitForTimeout(200);
		await expect(waffle.page.locator('[data-testid="origin-plane-front"]')).not.toBeVisible();

		// Re-expand
		await toggle.click();
		await waffle.page.waitForTimeout(200);
		await expect(waffle.page.locator('[data-testid="origin-plane-front"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="origin-plane-top"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="origin-plane-right"]')).toBeVisible();
	});

	test('expand icon changes between expanded and collapsed states', async ({ waffle }) => {
		const toggle = waffle.page.locator('[data-testid="origin-toggle"]');
		const expandIcon = toggle.locator('.expand-icon');

		// Expanded by default: should show down-pointing triangle
		const expandedText = await expandIcon.textContent();
		expect(expandedText).toContain('\u25BE');

		// Collapse
		await toggle.click();
		await waffle.page.waitForTimeout(200);

		// Collapsed: should show right-pointing triangle
		const collapsedText = await expandIcon.textContent();
		expect(collapsedText).toContain('\u25B8');
	});
});

test.describe('origin plane selection', () => {
	test('clicking plane selects it', async ({ waffle }) => {
		const front = waffle.page.locator('[data-testid="origin-plane-front"]');
		await front.click();
		await waffle.page.waitForTimeout(200);

		await expect(front).toHaveClass(/selected/);
	});

	test('clicking different plane changes selection', async ({ waffle }) => {
		const front = waffle.page.locator('[data-testid="origin-plane-front"]');
		const top = waffle.page.locator('[data-testid="origin-plane-top"]');

		// Select front
		await front.click();
		await waffle.page.waitForTimeout(200);
		await expect(front).toHaveClass(/selected/);

		// Select top
		await top.click();
		await waffle.page.waitForTimeout(200);
		await expect(top).toHaveClass(/selected/);

		// Front should no longer be selected
		const frontClasses = await front.getAttribute('class');
		expect(frontClasses).not.toContain('selected');
	});

	test('selection reflected in __waffle API', async ({ waffle }) => {
		const front = waffle.page.locator('[data-testid="origin-plane-front"]');
		await front.click();
		await waffle.page.waitForTimeout(200);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(1);
		expect(refs[0].anchor.type).toBe('DatumPlane');
	});
});

test.describe('origin plane names and testids', () => {
	test('plane names are Front, Top, Right', async ({ waffle }) => {
		const frontLabel = waffle.page.locator('[data-testid="origin-plane-front"] .tree-label');
		const topLabel = waffle.page.locator('[data-testid="origin-plane-top"] .tree-label');
		const rightLabel = waffle.page.locator('[data-testid="origin-plane-right"] .tree-label');

		await expect(frontLabel).toHaveText('Front');
		await expect(topLabel).toHaveText('Top');
		await expect(rightLabel).toHaveText('Right');
	});

	test('all three origin plane testids exist', async ({ waffle }) => {
		const front = waffle.page.locator('[data-testid="origin-plane-front"]');
		const top = waffle.page.locator('[data-testid="origin-plane-top"]');
		const right = waffle.page.locator('[data-testid="origin-plane-right"]');

		await expect(front).toBeAttached();
		await expect(top).toBeAttached();
		await expect(right).toBeAttached();
	});
});
