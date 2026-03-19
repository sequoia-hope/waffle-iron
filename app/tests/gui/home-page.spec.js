import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, makeTestDocument } from './helpers/waffle-test.js';

rawTest.describe('Home page', () => {
	rawTest('renders header and new document button', async ({ page }) => {
		await page.goto('/home');
		await expect(page.locator('[data-testid="home-header"]')).toBeVisible();
		await expect(page.locator('[data-testid="new-document-btn"]')).toBeVisible();
		await expect(page.locator('[data-testid="new-document-btn"]')).toContainText('New Document');
	});

	rawTest('shows empty state when no documents', async ({ page }) => {
		// Clear any leftover state
		await page.goto('/home');
		await page.evaluate(() => {
			localStorage.clear();
			return new Promise((resolve, reject) => {
				const del = indexedDB.deleteDatabase('waffle-iron');
				del.onsuccess = () => resolve();
				del.onerror = () => resolve(); // proceed even on error
				del.onblocked = () => resolve();
			});
		});
		// Reload with clean state
		await page.goto('/home');
		await expect(page.locator('[data-testid="empty-state"]')).toBeVisible({ timeout: 15000 });
		await expect(page.locator('[data-testid="empty-state"]')).toContainText('No documents yet');
	});

	rawTest('shows seeded documents as cards', async ({ page }) => {
		const doc = makeTestDocument({ id: 'seed0001', name: 'My Test Part' });
		await page.goto('/home');
		await seedDocument(page, doc);
		// Reload to pick up seeded doc
		await page.goto('/home');
		await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });
		await expect(page.locator('[data-testid="document-card"]')).toContainText('My Test Part');
	});

	rawTest('clicking card navigates to doc route', async ({ page }) => {
		const doc = makeTestDocument({ id: 'nav00001', name: 'Nav Test' });
		await page.goto('/home');
		await seedDocument(page, doc);
		await page.goto('/home');
		await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });
		await page.locator('[data-testid="document-card"]').first().click();
		// Should navigate to /doc/nav00001 which redirects to /
		await page.waitForURL(url => url.pathname === '/' || url.pathname.startsWith('/doc/'), { timeout: 10000 });
	});

	rawTest('new document button creates doc and navigates', async ({ page }) => {
		await page.goto('/home');
		// Wait for loading
		await page.waitForTimeout(1000);
		await page.locator('[data-testid="new-document-btn"]').click();
		// Should navigate away from /home (to / or /doc/*)
		await page.waitForURL(url => url.pathname !== '/home', { timeout: 10000 });
	});
});
