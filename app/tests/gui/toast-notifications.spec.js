/**
 * Toast notification tests — verifies the ToastContainer renders correctly,
 * toasts appear/dismiss as expected, and engine errors surface as toasts.
 */
import { test, expect } from './helpers/waffle-test.js';

test.describe('toast notifications', () => {
	test('toast container present', async ({ waffle }) => {
		const container = waffle.page.locator('[data-testid="toast-container"]');
		await expect(container).toBeAttached();
	});

	test('showToast error renders', async ({ waffle }) => {
		const page = waffle.page;

		// Clear any existing toasts
		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('error', 'Test error message'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-error/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Test error message');
	});

	test('showToast success renders', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('success', 'Operation succeeded'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-success/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Operation succeeded');
	});

	test('showToast info renders', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('info', 'Informational notice'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-info/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Informational notice');
	});

	test('showToast warning renders', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('warning', 'Warning notice'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-warning/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Warning notice');
	});

	test('toast close button dismisses', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show a toast with a long duration so it does not auto-dismiss
		const id = await page.evaluate(() => window.__waffle.showToast('info', 'Dismiss me', 30000));

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });

		// Click the close button
		const closeBtn = page.locator(`[data-testid="toast-close-${id}"]`);
		await expect(closeBtn).toBeVisible();
		await closeBtn.click();

		// Toast should be removed from the DOM
		await expect(toast).not.toBeAttached({ timeout: 3000 });
	});

	test('multiple toasts stack', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show 3 toasts with long duration
		await page.evaluate(() => {
			window.__waffle.showToast('error', 'First toast', 30000);
			window.__waffle.showToast('warning', 'Second toast', 30000);
			window.__waffle.showToast('info', 'Third toast', 30000);
		});

		// Wait for the toasts to render
		await page.waitForTimeout(300);

		const container = page.locator('[data-testid="toast-container"]');
		const toastItems = container.locator('.toast');
		await expect(toastItems).toHaveCount(3);
	});

	test('toast auto-dismisses', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show a toast with a short 500ms duration
		const id = await page.evaluate(() => window.__waffle.showToast('info', 'Auto-dismiss test', 500));

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });

		// Wait for auto-dismiss — should disappear within 1500ms
		await expect(toast).not.toBeAttached({ timeout: 1500 });
	});

	test('engine error triggers toast', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Trigger an error via showToast (simulating engine error path)
		await page.evaluate(() => window.__waffle.showToast('error', 'Simulated engine error'));

		// Verify the error toast appeared in DOM
		const errorToast = page.locator('.toast-error');
		await expect(errorToast).toBeVisible({ timeout: 2000 });
		await expect(errorToast).toContainText('Simulated engine error');

		// Verify via API
		const toasts = await page.evaluate(() => window.__waffle.getToasts());
		const hasError = toasts.some(t => t.level === 'error');
		expect(hasError).toBe(true);
	});

	test('programmatic dismiss works', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show several toasts with long duration
		await page.evaluate(() => {
			window.__waffle.showToast('error', 'Toast A', 30000);
			window.__waffle.showToast('info', 'Toast B', 30000);
			window.__waffle.showToast('success', 'Toast C', 30000);
		});

		await page.waitForTimeout(300);

		// Verify they are present
		let toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.length).toBe(3);

		// Dismiss all
		await page.evaluate(() => window.__waffle.dismissAllToasts());
		await page.waitForTimeout(300);

		// Verify all are gone
		toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.length).toBe(0);

		// Verify no toast elements remain in the DOM
		const container = waffle.page.locator('[data-testid="toast-container"]');
		const toastItems = container.locator('.toast');
		await expect(toastItems).toHaveCount(0);
	});
});
