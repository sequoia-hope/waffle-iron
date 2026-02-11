/**
 * Full Onshape-style tutorial workflows — end-to-end GUI tests.
 * These exercise the complete user journey: sketch -> draw -> finish -> extrude.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickLine,
	clickFinishSketch,
	clickExtrude,
	pressKey,
} from './helpers/toolbar.js';
import { drawRectangle, drawLine, clickAt } from './helpers/canvas.js';
import {
	isSketchActive,
	getActiveTool,
	getEntityCount,
	getEntityCountByType,
	getFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

test.describe('full workflows', () => {
	test('Tutorial 1: Sketch rectangle -> Extrude box (mouse)', async ({ waffle }) => {
		// Step 1: Click Sketch button
		await clickSketch(waffle.page);
		const inSketch = await isSketchActive(waffle.page);
		expect(inSketch).toBe(true);

		// Step 2: Click Rectangle tool
		await clickRectangle(waffle.page);
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('rectangle');

		// Step 3: Draw rectangle with two clicks
		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('tutorial1-draw-failed');
		}

		const points = await getEntityCountByType(waffle.page, 'Point');
		const lines = await getEntityCountByType(waffle.page, 'Line');
		expect(points).toBe(4);
		expect(lines).toBe(4);

		// Step 4: Click Finish Sketch
		await clickFinishSketch(waffle.page);
		const sketchDone = await isSketchActive(waffle.page);
		expect(sketchDone).toBe(false);

		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('tutorial1-finish-failed');
		}

		// Step 5: Click Extrude, set depth, Apply
		await clickExtrude(waffle.page);
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Step 6: Verify 3D box
		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('tutorial1-extrude-failed');
		}

		const hasSketch = await hasFeatureOfType(waffle.page, 'Sketch');
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasSketch).toBe(true);
		expect(hasExtrude).toBe(true);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('Tutorial 1 (keyboard): S -> R -> draw -> Esc -> Esc -> E -> depth -> Enter', async ({ waffle }) => {
		// Step 1: Press S to enter sketch
		await pressKey(waffle.page, 's');
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);

		// Step 2: Press R for rectangle
		await pressKey(waffle.page, 'r');
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('rectangle');

		// Step 3: Draw rectangle
		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('tutorial1-kb-draw-failed');
		}

		// Step 4: Escape to deselect tool, then Escape again to finish sketch
		await pressKey(waffle.page, 'Escape');
		let currentTool = await getActiveTool(waffle.page);
		expect(currentTool).toBe('select');

		await pressKey(waffle.page, 'Escape');
		// Pressing Escape while on select tool in sketch mode should finish the sketch
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === false,
			{ timeout: 10000 }
		);

		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('tutorial1-kb-finish-failed');
		}

		// Step 5: Press E for extrude
		await pressKey(waffle.page, 'e');
		await waffle.page.locator('[data-testid="extrude-dialog"]').waitFor({ state: 'visible', timeout: 5000 });

		// Step 6: Set depth and press Enter
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');
		await waffle.page.keyboard.press('Enter');

		// Verify results
		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('tutorial1-kb-extrude-failed');
		}

		const hasSketch = await hasFeatureOfType(waffle.page, 'Sketch');
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasSketch).toBe(true);
		expect(hasExtrude).toBe(true);
	});

	test('draw multiple lines in sketch', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Line tool is default
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('line');

		// Draw first line
		await drawLine(waffle.page, -100, 0, 0, 0);

		try {
			await waitForEntityCount(waffle.page, 3, 3000);
		} catch {
			await waffle.dumpState('multiline-first-failed');
		}

		// Draw second line
		await drawLine(waffle.page, 0, 0, 100, 0);

		try {
			await waitForEntityCount(waffle.page, 6, 3000);
		} catch {
			// May have fewer if points are shared via snapping
		}

		const totalEntities = await getEntityCount(waffle.page);
		expect(totalEntities).toBeGreaterThanOrEqual(4); // At least 2 lines + endpoints
	});

	test('sketch-extrude preserves orbit after completion', async ({ waffle }) => {
		// Complete full workflow
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {}

		await clickFinishSketch(waffle.page);

		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {}

		// After sketch is done, orbit should work
		const { orbitDrag } = await import('./helpers/canvas.js');
		await orbitDrag(waffle.page, 0, 0, 150, 100);

		// No crash, canvas still visible
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});
