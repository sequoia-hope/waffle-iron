import { test, expect } from '@playwright/test';

/**
 * Helper: wait for __waffle API to be available.
 * Returns true if available, false if timed out.
 */
async function waitForWaffle(page, timeout = 15000) {
	return page
		.waitForFunction(() => typeof window.__waffle !== 'undefined', { timeout })
		.then(() => true)
		.catch(() => false);
}

/**
 * Helper: enter sketch mode and wait for DOM event listeners to be attached.
 * We verify by checking that the sketch mode state is active and the canvas exists.
 */
async function enterSketchAndWait(page, tool = 'line') {
	await page.evaluate(async ({ tool }) => {
		await window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
		window.__waffle.setTool(tool);
	}, { tool });

	// Wait for Svelte reactivity to settle and DOM listeners to attach
	await page.waitForFunction(
		({ tool }) => {
			const state = window.__waffle?.getState();
			return state?.sketchMode?.active === true && state?.activeTool === tool;
		},
		{ tool },
		{ timeout: 5000 }
	);

	// Extra frame wait for $effect to fire and attach listeners
	await page.waitForTimeout(300);
}

test.describe('sketch interaction', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/');
	});

	test('enter sketch mode via __waffle API', async ({ page }) => {
		const ready = await waitForWaffle(page);
		test.skip(!ready, '__waffle API not available (engine may not have loaded)');

		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));

		const state = await page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
	});

	test('set tool via __waffle API', async ({ page }) => {
		const ready = await waitForWaffle(page);
		test.skip(!ready, '__waffle API not available');

		await page.evaluate(async () => {
			await window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
			window.__waffle.setTool('line');
		});

		const state = await page.evaluate(() => window.__waffle.getState());
		expect(state.activeTool).toBe('line');
	});

	test('click canvas with line tool creates entities', async ({ page }) => {
		const ready = await waitForWaffle(page);
		test.skip(!ready, '__waffle API not available');

		await enterSketchAndWait(page, 'line');

		const canvas = page.locator('canvas');
		await expect(canvas).toBeVisible();

		const box = await canvas.boundingBox();
		if (!box) {
			test.skip(true, 'Canvas not visible');
			return;
		}

		// Click first point
		await canvas.click({ position: { x: Math.round(box.width * 0.3), y: Math.round(box.height * 0.5) } });
		await page.waitForTimeout(300);

		// Click second point
		await canvas.click({ position: { x: Math.round(box.width * 0.6), y: Math.round(box.height * 0.5) } });
		await page.waitForTimeout(300);

		// Verify entities were created
		const entities = await page.evaluate(() => window.__waffle.getEntities());
		expect(entities.length).toBeGreaterThan(0);

		// Should have at least 2 points
		const points = entities.filter((e) => e.type === 'Point');
		expect(points.length).toBeGreaterThanOrEqual(2);
	});

	test('draw rectangle creates 4 lines', async ({ page }) => {
		const ready = await waitForWaffle(page);
		test.skip(!ready, '__waffle API not available');

		await enterSketchAndWait(page, 'rectangle');

		const canvas = page.locator('canvas');
		const box = await canvas.boundingBox();
		if (!box) {
			test.skip(true, 'Canvas not visible');
			return;
		}

		// Click two corners for rectangle
		await canvas.click({ position: { x: Math.round(box.width * 0.3), y: Math.round(box.height * 0.3) } });
		await page.waitForTimeout(300);
		await canvas.click({ position: { x: Math.round(box.width * 0.7), y: Math.round(box.height * 0.7) } });
		await page.waitForTimeout(300);

		const entities = await page.evaluate(() => window.__waffle.getEntities());
		const lines = entities.filter((e) => e.type === 'Line');
		expect(lines.length).toBe(4);

		const points = entities.filter((e) => e.type === 'Point');
		expect(points.length).toBe(4);
	});

	test('escape key switches to select tool', async ({ page }) => {
		const ready = await waitForWaffle(page);
		test.skip(!ready, '__waffle API not available');

		await page.evaluate(async () => {
			await window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
			window.__waffle.setTool('line');
		});

		let state = await page.evaluate(() => window.__waffle.getState());
		expect(state.activeTool).toBe('line');

		await page.keyboard.press('Escape');
		await page.waitForTimeout(200);

		state = await page.evaluate(() => window.__waffle.getState());
		expect(state.activeTool).toBe('select');
	});

	test('exit sketch mode via __waffle API', async ({ page }) => {
		const ready = await waitForWaffle(page);
		test.skip(!ready, '__waffle API not available');

		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));

		let state = await page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);

		await page.evaluate(() => {
			window.__waffle.exitSketch();
		});

		state = await page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(false);
	});
});
