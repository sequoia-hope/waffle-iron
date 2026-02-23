/**
 * Keyboard shortcut tests — verifies all app-level and sketch-mode shortcuts
 * including tool activation, undo/redo, Escape behavior, and input focus suppression.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	pressKey,
} from './helpers/toolbar.js';
import { drawLine, drawRectangle } from './helpers/canvas.js';
import {
	isSketchActive,
	getActiveTool,
	getEntityCount,
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
} from './helpers/state.js';

test.describe('app-level shortcuts', () => {
	test('S opens sketch plane selection prompt', async ({ waffle }) => {
		await pressKey(waffle.page, 's');

		const prompt = waffle.page.locator('[data-testid="sketch-plane-prompt"]');
		await expect(prompt).toBeVisible({ timeout: 3000 });
	});

	test('E opens extrude dialog when sketch feature exists', async ({ waffle }) => {
		// Create a sketch with a rectangle first
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		await waitForEntityCount(waffle.page, 8, 3000);
		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		// Press 'e' to open extrude dialog
		await pressKey(waffle.page, 'e');

		const dialog = waffle.page.locator('[data-testid="extrude-dialog"]');
		await expect(dialog).toBeVisible({ timeout: 5000 });
	});
});

test.describe('sketch-mode tool shortcuts', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		// Sketch mode starts with line tool by default; switch to select first
		await pressKey(waffle.page, 'Escape');
		expect(await getActiveTool(waffle.page)).toBe('select');
	});

	test('L activates line tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'l');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('line');
	});

	test('R activates rectangle tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'r');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('rectangle');
	});

	test('C activates circle tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'c');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('circle');
	});

	test('A activates arc tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'a');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('arc');
	});

	test('D activates dimension tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'd');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('dimension');
	});
});

test.describe('Escape key behavior', () => {
	test('Escape returns to select from drawing tool', async ({ waffle }) => {
		await clickSketch(waffle.page);
		// Should be in line tool by default
		expect(await getActiveTool(waffle.page)).toBe('line');

		await pressKey(waffle.page, 'Escape');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('select');
	});

	test('Escape from select exits sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);
		expect(await isSketchActive(waffle.page)).toBe(true);

		// First Escape: line tool → select
		await pressKey(waffle.page, 'Escape');
		expect(await getActiveTool(waffle.page)).toBe('select');

		// Second Escape: select → exit sketch mode
		await pressKey(waffle.page, 'Escape');
		await waffle.page.waitForTimeout(500);

		const active = await isSketchActive(waffle.page);
		expect(active).toBe(false);
	});
});

test.describe('undo/redo shortcuts', () => {
	test('Ctrl+Z triggers undo', async ({ waffle }) => {
		// Create a sketch feature
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		await waitForEntityCount(waffle.page, 8, 3000);
		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		expect(await getFeatureCount(waffle.page)).toBe(1);

		// Ctrl+Z to undo
		await waffle.page.keyboard.press('Control+z');
		await waffle.page.waitForTimeout(500);

		expect(await getFeatureCount(waffle.page)).toBe(0);
	});

	test('Ctrl+Shift+Z triggers redo', async ({ waffle }) => {
		// Create a sketch feature
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		await waitForEntityCount(waffle.page, 8, 3000);
		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		// Undo
		await waffle.page.keyboard.press('Control+z');
		await waffle.page.waitForTimeout(500);
		expect(await getFeatureCount(waffle.page)).toBe(0);

		// Redo
		await waffle.page.keyboard.press('Control+Shift+z');
		await waffle.page.waitForTimeout(500);

		await waitForFeatureCount(waffle.page, 1, 5000);

		expect(await getFeatureCount(waffle.page)).toBe(1);
	});
});

test.describe('shortcut suppression', () => {
	test('shortcuts ignored when input is focused', async ({ waffle }) => {
		// Create sketch + extrude to get a dialog with a number input
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		await waitForEntityCount(waffle.page, 8, 3000);
		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		// Open extrude dialog
		await clickExtrude(waffle.page);

		// Focus the depth input
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.focus();
		await waffle.page.waitForTimeout(100);

		// Record the active tool before pressing shortcut key
		const toolBefore = await getActiveTool(waffle.page);

		// Press 'l' — should NOT switch to line tool (input is focused)
		await waffle.page.keyboard.press('l');
		await waffle.page.waitForTimeout(200);

		// We should still be outside sketch mode (extrude dialog is up)
		const sketchActive = await isSketchActive(waffle.page);
		expect(sketchActive).toBe(false);

		// The tool should not have changed
		const toolAfter = await getActiveTool(waffle.page);
		expect(toolAfter).toBe(toolBefore);
	});
});
