import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, makeTestDocument } from './helpers/waffle-test.js';

rawTest.describe('Document routing', () => {
	rawTest('/home shows home page', async ({ page }) => {
		await page.goto('/home');
		await expect(page.locator('[data-testid="home-page"]')).toBeVisible({ timeout: 10000 });
	});

	rawTest('/doc/{id} loads seeded document and redirects', async ({ page }) => {
		const doc = makeTestDocument({ id: 'route001', name: 'Routed Doc' });
		// Seed document first (go to any page to get IndexedDB access)
		await page.goto('/home');
		await seedDocument(page, doc);
		// Navigate to document route
		await page.goto('/doc/route001');
		// Should redirect to / (editor)
		await page.waitForURL('/', { timeout: 15000 });
	});

	rawTest('toolbar home button navigates to /home', async ({ page }) => {
		await page.goto('/');
		// Wait for app to load
		await page.waitForFunction(() => typeof window.__waffle !== 'undefined', { timeout: 30000 });
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });
		const homeBtn = page.locator('[data-testid="toolbar-btn-home"]');
		await expect(homeBtn).toBeVisible();
		await homeBtn.click();
		await page.waitForURL('/home', { timeout: 10000 });
		await expect(page.locator('[data-testid="home-page"]')).toBeVisible();
	});
});
