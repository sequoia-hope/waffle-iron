import { test, expect } from './helpers/waffle-test.js';

test.describe('Tab bar', () => {
	test('shows default Part 1 tab', async ({ waffle }) => {
		const page = waffle.page;
		const tabBar = page.locator('[data-testid="tab-bar"]');
		await expect(tabBar).toBeVisible();
		// Should have at least one tab with "Part 1"
		await expect(tabBar.locator('.tab')).toHaveCount(1);
		await expect(tabBar.locator('.tab').first()).toContainText('Part 1');
	});

	test('add button creates new tab', async ({ waffle }) => {
		const page = waffle.page;
		const addBtn = page.locator('[data-testid="tab-add"]');
		await addBtn.click();
		const tabs = page.locator('[data-testid="tab-bar"] .tab');
		await expect(tabs).toHaveCount(2);
		// New tab should have "Part 2" name
		await expect(tabs.nth(1)).toContainText('Part 2');
	});

	test('double-click renames tab', async ({ waffle }) => {
		const page = waffle.page;
		const tab = page.locator('[data-testid="tab-bar"] .tab').first();
		await tab.dblclick();
		// Should show rename input
		const input = page.locator('.tab-rename-input');
		await expect(input).toBeVisible();
		await input.fill('My Custom Name');
		await input.press('Enter');
		await expect(tab).toContainText('My Custom Name');
	});

	test('close button removes tab when multiple tabs exist', async ({ waffle }) => {
		const page = waffle.page;
		// Add a second tab first
		await page.locator('[data-testid="tab-add"]').click();
		const tabs = page.locator('[data-testid="tab-bar"] .tab');
		await expect(tabs).toHaveCount(2);
		// Close the second tab
		const closeBtn = page.locator('[data-testid="tab-bar"] .tab-close').last();
		await closeBtn.click();
		await expect(tabs).toHaveCount(1);
	});

	test('tab switching changes active class', async ({ waffle }) => {
		const page = waffle.page;
		// Add second tab
		await page.locator('[data-testid="tab-add"]').click();
		const tabs = page.locator('[data-testid="tab-bar"] .tab');
		// Click second tab
		await tabs.nth(1).click();
		await expect(tabs.nth(1)).toHaveClass(/active/);
		// Click first tab back
		await tabs.nth(0).click();
		await expect(tabs.nth(0)).toHaveClass(/active/);
	});
});
