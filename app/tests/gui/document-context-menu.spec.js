import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, makeTestDocument } from './helpers/waffle-test.js';

rawTest.describe('Document context menu', () => {
  rawTest('right-click shows rename and delete options', async ({ page }) => {
    const doc = makeTestDocument({ id: 'ctx00001', name: 'Right Click Me' });
    await page.goto('/home');
    await seedDocument(page, doc);
    await page.goto('/home');
    await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });

    await page.locator('[data-testid="document-card"]').first().click({ button: 'right' });
    await expect(page.locator('[data-testid="doc-context-menu"]')).toBeVisible();
    await expect(page.locator('[data-testid="doc-ctx-rename"]')).toBeVisible();
    await expect(page.locator('[data-testid="doc-ctx-delete"]')).toBeVisible();
  });

  rawTest('rename via context menu updates card name', async ({ page }) => {
    const doc = makeTestDocument({ id: 'ren00001', name: 'Old Name' });
    await page.goto('/home');
    await seedDocument(page, doc);
    await page.goto('/home');
    await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });

    // Right-click → Rename
    await page.locator('[data-testid="document-card"]').first().click({ button: 'right' });
    await page.locator('[data-testid="doc-ctx-rename"]').click();

    // Should show input
    const input = page.locator('[data-testid="doc-rename-input"]');
    await expect(input).toBeVisible();
    await input.fill('New Name');
    await input.press('Enter');

    // Card should update
    await page.waitForTimeout(500);
    await expect(page.locator('.card-name')).toContainText('New Name');
  });

  rawTest('delete via context menu removes card', async ({ page }) => {
    const doc = makeTestDocument({ id: 'del00001', name: 'Delete Me' });
    await page.goto('/home');
    await seedDocument(page, doc);
    await page.goto('/home');
    await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });

    // Accept the confirm dialog
    page.on('dialog', dialog => dialog.accept());

    // Right-click → Delete
    await page.locator('[data-testid="document-card"]').first().click({ button: 'right' });
    await page.locator('[data-testid="doc-ctx-delete"]').click();

    // Card should disappear, empty state should show
    await expect(page.locator('[data-testid="empty-state"]')).toBeVisible({ timeout: 5000 });
  });
});
