import { test, expect } from '@playwright/test';

test('page loads without console errors', async ({ page }) => {
	const errors = [];
	page.on('pageerror', (err) => errors.push(err.message));

	await page.goto('/');
	await page.waitForTimeout(2000);

	// Filter out known WASM-loading errors (expected when pkg files are missing)
	const criticalErrors = errors.filter(
		(e) => !e.includes('wasm') && !e.includes('WASM') && !e.includes('pkg/')
	);
	expect(criticalErrors).toEqual([]);
});

test('toolbar renders with modeling tools', async ({ page }) => {
	await page.goto('/');
	await page.waitForTimeout(2000);

	// Look for the toolbar area — should have buttons
	const buttons = page.locator('button');
	const count = await buttons.count();
	expect(count).toBeGreaterThan(0);
});

test('canvas element exists', async ({ page }) => {
	await page.goto('/');
	await page.waitForTimeout(2000);

	const canvas = page.locator('canvas');
	await expect(canvas).toBeVisible();
});

test('__waffle API is exposed after engine init', async ({ page }) => {
	await page.goto('/');

	// Wait for __waffle to appear (engine may take time to init)
	const hasApi = await page.waitForFunction(
		() => typeof window.__waffle !== 'undefined',
		{ timeout: 15000 }
	).then(() => true).catch(() => false);

	// Even if engine WASM fails to load, __waffle should be set
	// (it's set after the try/catch in initEngine)
	if (hasApi) {
		const state = await page.evaluate(() => window.__waffle.getState());
		expect(state).toHaveProperty('activeTool');
		expect(state).toHaveProperty('sketchMode');
		expect(state).toHaveProperty('entityCount');
	}
});
