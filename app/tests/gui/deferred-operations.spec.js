/**
 * Deferred operations tests — verifies fillet, chamfer, and shell dialogs
 * display deferred warning banners and have disabled Apply buttons.
 *
 * These operations are DEFERRED INDEFINITELY. The dialogs open but
 * prevent the user from applying changes.
 */
import { test, expect } from './helpers/waffle-test.js';

test.describe('deferred operations', () => {
	test.describe('fillet dialog', () => {
		test('opens with deferred warning', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-fillet"]').click();

			const dialog = waffle.page.locator('[data-testid="fillet-dialog"]');
			await expect(dialog).toBeVisible();

			const warning = waffle.page.locator('[data-testid="fillet-deferred-warning"]');
			await expect(warning).toBeVisible();
			await expect(warning).toContainText('not yet available');
		});

		test('Apply button is disabled', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-fillet"]').click();

			const apply = waffle.page.locator('[data-testid="fillet-apply"]');
			await expect(apply).toBeDisabled();
		});

		test('Cancel closes dialog', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-fillet"]').click();

			const dialog = waffle.page.locator('[data-testid="fillet-dialog"]');
			await expect(dialog).toBeVisible();

			await waffle.page.locator('[data-testid="fillet-cancel"]').click();
			await expect(dialog).not.toBeVisible();
		});

		test('Escape key closes dialog', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-fillet"]').click();

			const dialog = waffle.page.locator('[data-testid="fillet-dialog"]');
			await expect(dialog).toBeVisible();

			await waffle.page.keyboard.press('Escape');
			await expect(dialog).not.toBeVisible();
		});
	});

	test.describe('chamfer dialog', () => {
		test('opens with deferred warning', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-chamfer"]').click();

			const dialog = waffle.page.locator('[data-testid="chamfer-dialog"]');
			await expect(dialog).toBeVisible();

			const warning = waffle.page.locator('[data-testid="chamfer-deferred-warning"]');
			await expect(warning).toBeVisible();
			await expect(warning).toContainText('not yet available');
		});

		test('Apply button is disabled', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-chamfer"]').click();

			const apply = waffle.page.locator('[data-testid="chamfer-apply"]');
			await expect(apply).toBeDisabled();
		});

		test('Cancel closes dialog', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-chamfer"]').click();

			const dialog = waffle.page.locator('[data-testid="chamfer-dialog"]');
			await expect(dialog).toBeVisible();

			await waffle.page.locator('[data-testid="chamfer-cancel"]').click();
			await expect(dialog).not.toBeVisible();
		});

		test('Escape key closes dialog', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-chamfer"]').click();

			const dialog = waffle.page.locator('[data-testid="chamfer-dialog"]');
			await expect(dialog).toBeVisible();

			await waffle.page.keyboard.press('Escape');
			await expect(dialog).not.toBeVisible();
		});
	});

	test.describe('shell dialog', () => {
		test('opens with deferred warning', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-shell"]').click();

			const dialog = waffle.page.locator('[data-testid="shell-dialog"]');
			await expect(dialog).toBeVisible();

			const warning = waffle.page.locator('[data-testid="shell-deferred-warning"]');
			await expect(warning).toBeVisible();
			await expect(warning).toContainText('not yet available');
		});

		test('Apply button is disabled', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-shell"]').click();

			const apply = waffle.page.locator('[data-testid="shell-apply"]');
			await expect(apply).toBeDisabled();
		});

		test('Cancel closes dialog', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-shell"]').click();

			const dialog = waffle.page.locator('[data-testid="shell-dialog"]');
			await expect(dialog).toBeVisible();

			await waffle.page.locator('[data-testid="shell-cancel"]').click();
			await expect(dialog).not.toBeVisible();
		});

		test('Escape key closes dialog', async ({ waffle }) => {
			await waffle.page.locator('[data-testid="toolbar-btn-shell"]').click();

			const dialog = waffle.page.locator('[data-testid="shell-dialog"]');
			await expect(dialog).toBeVisible();

			await waffle.page.keyboard.press('Escape');
			await expect(dialog).not.toBeVisible();
		});
	});
});
