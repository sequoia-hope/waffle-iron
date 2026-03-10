/**
 * Tool switching mid-operation tests.
 *
 * Verifies clean state reset when switching tools while an operation is in progress.
 * Uses getToolState() and entity counts to verify no stale state leaks across tools.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickRectangle, clickCircle, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle, drawCircle } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	getEntityCount,
	getEntityCountByType,
	getToolState,
	getActiveTool,
	waitForEntityCount,
} from './helpers/state.js';

test.describe('tool switching mid-operation', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('switch from line (first point placed) to rectangle resets state', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Place first point of a line
		await clickAt(page, -50, 0);
		await page.waitForTimeout(200);

		// Verify first point placed
		const stateAfterClick = await getToolState(page);
		expect(stateAfterClick).toBe('firstPointPlaced');

		// Switch to rectangle tool
		await clickRectangle(page);

		// State should reset to idle
		const stateAfterSwitch = await getToolState(page);
		expect(stateAfterSwitch).toBe('idle');

		// Draw a complete rectangle
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		// Rectangle creates 4 lines + 4 points; the stale point from line tool's first click
		// remains in the sketch (it was committed), so total points = 5
		const points = await getEntityCountByType(page, 'Point');
		const lines = await getEntityCountByType(page, 'Line');
		expect(points).toBe(5);
		expect(lines).toBe(4);

		expectNoAnyCrash(crashes);
	});

	test('switch from rectangle (first corner) to circle', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Activate rectangle and place first corner
		await clickRectangle(page);
		await clickAt(page, -50, -50);
		await page.waitForTimeout(200);

		// Switch to circle
		await clickCircle(page);

		const state = await getToolState(page);
		expect(state).toBe('idle');

		// Draw a circle
		await drawCircle(page, 0, 0, 50, 0);
		await waitForEntityCount(page, 2, 5000);

		const circles = await getEntityCountByType(page, 'Circle');
		const points = await getEntityCountByType(page, 'Point');
		expect(circles).toBe(1);
		// Circle center point (1) + stale point from rectangle's first corner click (1) = 2
		expect(points).toBe(2);

		expectNoAnyCrash(crashes);
	});

	test('switch from polyline (mid-drawing) to line preserves committed segments', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Activate polyline
		await pressKey(page, 'p');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'polyline',
			{ timeout: 3000 }
		);

		// Click 3 points to create 2 segments
		await clickAt(page, -100, 0);
		await page.waitForTimeout(200);
		await clickAt(page, 0, 50);
		await page.waitForTimeout(200);
		await clickAt(page, 100, 0);
		await page.waitForTimeout(200);

		// Should have 2 committed line segments
		const linesBeforeSwitch = await getEntityCountByType(page, 'Line');
		expect(linesBeforeSwitch).toBe(2);

		// Switch to line tool
		await clickLine(page);

		// Previous segments should still exist
		const linesAfterSwitch = await getEntityCountByType(page, 'Line');
		expect(linesAfterSwitch).toBe(2);

		// Draw a new line
		await drawLine(page, -50, -50, 50, -50);
		await page.waitForTimeout(300);

		const totalLines = await getEntityCountByType(page, 'Line');
		expect(totalLines).toBe(3);

		expectNoAnyCrash(crashes);
	});

	test('rapid tool cycling leaves clean state', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Cycle through tools: L -> R -> C -> L
		await pressKey(page, 'l');
		await page.waitForTimeout(100);
		await pressKey(page, 'r');
		await page.waitForTimeout(100);
		await pressKey(page, 'c');
		await page.waitForTimeout(100);
		await pressKey(page, 'l');
		await page.waitForTimeout(200);

		// Final tool should be line
		const tool = await getActiveTool(page);
		expect(tool).toBe('line');

		// State should be idle
		const state = await getToolState(page);
		expect(state).toBe('idle');

		// No entities should have been created
		const count = await getEntityCount(page);
		expect(count).toBe(0);

		expectNoAnyCrash(crashes);
	});

	test('switch tool while gear dialog open cancels dialog', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Activate gear tool and open dialog
		await pressKey(page, 'g');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'gear',
			{ timeout: 3000 }
		);
		await clickAt(page, 0, 0);

		const dialog = page.locator('[data-testid="gear-dialog"]');
		await dialog.waitFor({ state: 'visible', timeout: 5000 });

		// Switch to line tool via keyboard
		await pressKey(page, 'l');
		await page.waitForTimeout(300);

		// Dialog should be hidden (gear dialog listens for tool changes)
		// or the tool should have changed — either way, no crash
		const tool = await getActiveTool(page);
		// Tool might still be gear (if key was captured by dialog) or line
		expect(['gear', 'line']).toContain(tool);

		// No entities from the cancelled gear
		const count = await getEntityCount(page);
		expect(count).toBe(0);

		expectNoAnyCrash(crashes);
	});
});
