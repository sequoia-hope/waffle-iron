/**
 * Screenshot baseline suite — captures and compares visual snapshots
 * of key app states. On first run, baselines are created. On subsequent
 * runs, screenshots are compared against the baseline.
 *
 * Uses the assertScreenshot helper which masks volatile elements
 * (status messages, timestamps) by default.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { assertScreenshot } from '../helpers/screenshot.js';
import { getCanvasBounds, clickAt, orbitDrag, zoom } from '../helpers/canvas.js';

test.describe('screenshot baselines — scene states', () => {
	test('empty scene (test box)', async ({ waffle }) => {
		await assertScreenshot(waffle.page, 'scene-empty.png');
	});

	test('scene after orbit', async ({ waffle }) => {
		await orbitDrag(waffle.page, 0, 0, 100, 50);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'scene-after-orbit.png');
	});

	test('scene after zoom in', async ({ waffle }) => {
		await zoom(waffle.page, -200);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'scene-zoomed-in.png');
	});

	test('scene after zoom out', async ({ waffle }) => {
		await zoom(waffle.page, 300);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'scene-zoomed-out.png');
	});

	test('scene after fit-all', async ({ waffle }) => {
		// Orbit away first
		await orbitDrag(waffle.page, 0, 0, 150, 100);
		await waffle.page.waitForTimeout(200);
		// Then fit-all
		await waffle.page.keyboard.press('f');
		await waffle.page.waitForTimeout(500);
		await assertScreenshot(waffle.page, 'scene-fit-all.png');
	});
});

test.describe('screenshot baselines — selection states', () => {
	test('face hover', async ({ waffle }) => {
		const bounds = await getCanvasBounds(waffle.page);
		// Move to center of test box to trigger hover
		await waffle.page.mouse.move(bounds.centerX, bounds.centerY);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'selection-face-hover.png');
	});

	test('face selected', async ({ waffle }) => {
		await clickAt(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'selection-face-selected.png');
	});

	test('no selection (click empty)', async ({ waffle }) => {
		// Click far from model to clear
		const bounds = await getCanvasBounds(waffle.page);
		await waffle.page.mouse.click(bounds.x + 10, bounds.y + 10);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'selection-none.png');
	});
});

test.describe('screenshot baselines — sketch mode', () => {
	test('sketch mode active', async ({ waffle }) => {
		const sketchBtn = waffle.page.locator('[data-testid="toolbar-btn-sketch"]');
		if (await sketchBtn.isVisible()) {
			await sketchBtn.click();
			await waffle.page.waitForTimeout(500);
		}
		await assertScreenshot(waffle.page, 'sketch-mode-active.png');
	});

	test('sketch mode with grid', async ({ waffle }) => {
		const sketchBtn = waffle.page.locator('[data-testid="toolbar-btn-sketch"]');
		if (await sketchBtn.isVisible()) {
			await sketchBtn.click();
			await waffle.page.waitForTimeout(500);
		}
		// The sketch grid should be visible
		await assertScreenshot(waffle.page, 'sketch-mode-grid.png');
	});
});

test.describe('screenshot baselines — datum planes', () => {
	test('datum planes default view', async ({ waffle }) => {
		// Datum planes should be visible in the initial scene
		await assertScreenshot(waffle.page, 'datum-planes-default.png');
	});

	test('datum planes after orbit', async ({ waffle }) => {
		await orbitDrag(waffle.page, 0, 0, 80, -60);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'datum-planes-orbited.png');
	});
});

test.describe('screenshot baselines — view angles', () => {
	test('front view', async ({ waffle }) => {
		// Reset to a known orientation first
		await waffle.page.keyboard.press('f');
		await waffle.page.waitForTimeout(500);
		await assertScreenshot(waffle.page, 'view-front.png');
	});

	test('top view', async ({ waffle }) => {
		// Orbit to look from top
		await orbitDrag(waffle.page, 0, 0, 0, -200);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'view-top.png');
	});

	test('isometric view', async ({ waffle }) => {
		await orbitDrag(waffle.page, 0, 0, 100, -80);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'view-isometric.png');
	});
});

test.describe('screenshot baselines — UI chrome', () => {
	test('toolbar visible', async ({ waffle }) => {
		await assertScreenshot(waffle.page, 'ui-toolbar.png');
	});

	test('feature tree panel', async ({ waffle }) => {
		await assertScreenshot(waffle.page, 'ui-feature-tree.png');
	});
});

test.describe('screenshot baselines — combined states', () => {
	test('sketch with rectangle drawn', async ({ waffle }) => {
		const sketchBtn = waffle.page.locator('[data-testid="toolbar-btn-sketch"]');
		if (await sketchBtn.isVisible()) {
			await sketchBtn.click();
			await waffle.page.waitForTimeout(500);
		}

		// Select rectangle tool if available
		const rectBtn = waffle.page.locator('[data-testid="sketch-tool-rectangle"]');
		if (await rectBtn.isVisible()) {
			await rectBtn.click();
			await waffle.page.waitForTimeout(200);
		}

		// Draw a rectangle
		await clickAt(waffle.page, -30, -30);
		await clickAt(waffle.page, 30, 30);
		await waffle.page.waitForTimeout(300);

		await assertScreenshot(waffle.page, 'sketch-rectangle-drawn.png');
	});

	test('zoomed in with selection', async ({ waffle }) => {
		// Zoom in
		await zoom(waffle.page, -200);
		await waffle.page.waitForTimeout(200);

		// Select center face
		await clickAt(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(300);

		await assertScreenshot(waffle.page, 'zoomed-selected.png');
	});

	test('orbited with edge highlight area', async ({ waffle }) => {
		await orbitDrag(waffle.page, 0, 0, 80, 40);
		await waffle.page.waitForTimeout(300);
		await assertScreenshot(waffle.page, 'orbited-edge-area.png');
	});

	test('after multi-step interaction', async ({ waffle }) => {
		// Orbit
		await orbitDrag(waffle.page, 0, 0, 50, -30);
		await waffle.page.waitForTimeout(200);
		// Zoom
		await zoom(waffle.page, -100);
		await waffle.page.waitForTimeout(200);
		// Select
		await clickAt(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(200);

		await assertScreenshot(waffle.page, 'multi-step-final.png');
	});
});
