/**
 * Entering sketch mode via GUI — clicking toolbar, keyboard shortcuts.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, pressKey, isToolbarButtonVisible } from './helpers/toolbar.js';
import { isSketchActive, getActiveTool } from './helpers/state.js';

test.describe('sketch entry via GUI', () => {
	test('clicking Sketch button enters sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const active = await isSketchActive(waffle.page);
		expect(active).toBe(true);
	});

	test('entering sketch mode switches toolbar to sketch tools', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Sketch tools should be visible
		expect(await isToolbarButtonVisible(waffle.page, 'line')).toBe(true);
		expect(await isToolbarButtonVisible(waffle.page, 'rectangle')).toBe(true);
		expect(await isToolbarButtonVisible(waffle.page, 'circle')).toBe(true);
		expect(await isToolbarButtonVisible(waffle.page, 'arc')).toBe(true);

		// Modeling tools should be hidden
		expect(await isToolbarButtonVisible(waffle.page, 'extrude')).toBe(false);
		expect(await isToolbarButtonVisible(waffle.page, 'revolve')).toBe(false);
	});

	test('entering sketch mode shows Finish Sketch button', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const finishBtn = waffle.page.locator('[data-testid="toolbar-btn-finish-sketch"]');
		await expect(finishBtn).toBeVisible();
	});

	test('S key enters sketch mode', async ({ waffle }) => {
		await pressKey(waffle.page, 's');

		const active = await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		).then(() => true).catch(() => false);

		expect(active).toBe(true);
	});

	test('entering sketch mode sets line tool as default', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('line');
	});

	test('status bar reflects sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const statusText = await waffle.page.locator('[data-testid="statusbar"]').textContent();
		expect(statusText).toContain('Sketch Mode');
	});
});
