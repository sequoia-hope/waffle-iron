/**
 * Completing a sketch — Finish Sketch button, feature tree, mode exit.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	isSketchActive,
	getActiveTool,
	getFeatureCount,
	hasFeatureOfType,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';
import { isToolbarButtonVisible } from './helpers/toolbar.js';

test.describe('sketch finish via GUI', () => {
	test('draw rectangle then Finish Sketch creates a Sketch feature', async ({ waffle }) => {
		// Enter sketch mode
		await clickSketch(waffle.page);

		// Switch to rectangle and draw
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('finish-sketch-draw-failed');
		}

		// Click Finish Sketch
		await clickFinishSketch(waffle.page);

		// Verify sketch mode exited
		const active = await isSketchActive(waffle.page);
		expect(active).toBe(false);

		// Verify feature tree has a Sketch feature
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('finish-sketch-feature-failed');
		}

		const hasSketch = await hasFeatureOfType(waffle.page, 'Sketch');
		expect(hasSketch).toBe(true);
	});

	test('after finishing sketch, toolbar returns to modeling tools', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			// Continue anyway to test finish behavior
		}

		await clickFinishSketch(waffle.page);

		// Modeling tools should be visible again
		const hasExtrude = await isToolbarButtonVisible(waffle.page, 'extrude');
		expect(hasExtrude).toBe(true);

		const hasSketch = await isToolbarButtonVisible(waffle.page, 'sketch');
		expect(hasSketch).toBe(true);

		// Sketch tools should be hidden
		const hasLine = await isToolbarButtonVisible(waffle.page, 'line');
		expect(hasLine).toBe(false);
	});

	test('after finishing sketch, tool resets to select', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {}

		await clickFinishSketch(waffle.page);

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('select');
	});

	test('orbit controls re-enable after exiting sketch', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Verify we're in sketch mode
		const active = await isSketchActive(waffle.page);
		expect(active).toBe(true);

		// Draw something and finish
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {}

		await clickFinishSketch(waffle.page);

		// After exit, orbit drag should work (no freeze)
		const { orbitDrag } = await import('./helpers/canvas.js');
		await orbitDrag(waffle.page, 0, 0, 100, 50);

		// Canvas should still be visible and responsive
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});
