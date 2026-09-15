/**
 * Home view document actions on touch screens: there is no right-click, so each
 * card has a ⋯ button that opens the same menu.
 *
 * Runs in the mobile-portrait and mobile-landscape Playwright projects.
 */
import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, makeTestDocument } from '../helpers/waffle-test.js';
import { assertElementWithinBounds } from '../helpers/mobile.js';

rawTest.describe('Mobile home document menu', () => {
	rawTest('⋯ button opens the document menu inside the viewport without opening the document', async ({ page }) => {
		const doc = makeTestDocument({ id: 'mob00001', name: 'Tap Menu' });
		await page.goto('/home');
		await seedDocument(page, doc);
		await page.goto('/home');
		await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });

		await page.locator('[data-testid="doc-menu-btn"]').first().tap();
		await expect(page.locator('[data-testid="doc-context-menu"]')).toBeVisible();
		await expect(page.locator('[data-testid="doc-ctx-export"]')).toBeVisible();
		await expect(page.locator('[data-testid="doc-ctx-rename"]')).toBeVisible();
		await expect(page.locator('[data-testid="doc-ctx-delete"]')).toBeVisible();
		await assertElementWithinBounds(page, '[data-testid="doc-context-menu"]', expect);
		// Tapping ⋯ must not navigate into the document.
		await page.waitForTimeout(400);
		await expect(page).toHaveURL(/\/home$/);
	});

	rawTest('export from the ⋯ menu downloads the stored .waffle verbatim', async ({ page }) => {
		const doc = makeTestDocument({ id: 'mob00002', name: 'Tap Export' });
		await page.goto('/home');
		await seedDocument(page, doc);
		await page.goto('/home');
		await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });

		await page.locator('[data-testid="doc-menu-btn"]').first().tap();
		const downloadPromise = page.waitForEvent('download');
		await page.locator('[data-testid="doc-ctx-export"]').tap();
		const download = await downloadPromise;

		expect(download.suggestedFilename()).toBe('Tap Export.waffle');
		const fs = await import('node:fs/promises');
		expect(await fs.readFile(await download.path(), 'utf8')).toBe(doc.json);
		await expect(page).toHaveURL(/\/home$/);
	});
});
