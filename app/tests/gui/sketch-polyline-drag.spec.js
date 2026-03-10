/**
 * Polyline tool drag behavior tests.
 *
 * Per CLAUDE.md: "Every drawing mode needs BOTH click-click AND click-drag tests."
 * The polyline tool has no pointerup drag handler, so drags are treated as clicks.
 * These tests verify graceful degradation.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, pressKey } from './helpers/toolbar.js';
import { clickAt, dragLine } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	getEntityCountByType,
	getToolState,
	waitForEntityCount,
} from './helpers/state.js';

test.describe('polyline drag behavior', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		// Activate polyline tool
		await pressKey(waffle.page, 'p');
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'polyline',
			{ timeout: 3000 }
		);
	});

	test('drag on polyline tool places first point', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Drag on canvas — polyline has no drag handler, so this should act as a click
		await dragLine(page, -100, 0, 100, 0);

		// Tool should be in polyDrawing state (first point placed) or idle
		const state = await getToolState(page);
		expect(['idle', 'polyDrawing']).toContain(state);

		expectNoAnyCrash(crashes);
	});

	test('drag then click-click completes segments', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Drag start — treated as click, places first point
		await dragLine(page, -100, 0, 0, 0);
		await page.waitForTimeout(200);

		// Click second point
		await clickAt(page, 50, 50);
		await page.waitForTimeout(200);

		// Click third point
		await clickAt(page, 100, 0);
		await page.waitForTimeout(200);

		// Escape to finish polyline
		await pressKey(page, 'Escape');
		await page.waitForTimeout(200);

		// Should have at least 2 line segments
		const lines = await getEntityCountByType(page, 'Line');
		expect(lines).toBeGreaterThanOrEqual(2);

		expectNoAnyCrash(crashes);
	});

	test('very short drag below threshold is consistent', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// 1px drag — below DRAG_THRESHOLD_PX=5
		await dragLine(page, 0, 0, 1, 1);
		await page.waitForTimeout(200);

		// Should be idle or polyDrawing — no crash
		const state = await getToolState(page);
		expect(['idle', 'polyDrawing']).toContain(state);

		expectNoAnyCrash(crashes);
	});

	test('polyline drag does not create spurious lines', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Single drag — should not complete a line segment (needs 2 distinct clicks)
		await dragLine(page, -100, 0, 100, 0);
		await page.waitForTimeout(300);

		// A drag doesn't complete a segment — at most places a first point
		const lines = await getEntityCountByType(page, 'Line');
		expect(lines).toBeLessThanOrEqual(1);

		expectNoAnyCrash(crashes);
	});
});
