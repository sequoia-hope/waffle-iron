/**
 * Arc tool regression tests — 3-click arc drawing, tool state transitions,
 * entity accumulation, cancel behavior, and tool switching.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickArc, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, drawArc } from './helpers/canvas.js';
import {
	getActiveTool,
	getEntityCount,
	getEntityCountByType,
	getEntities,
	waitForEntityCount,
	getToolState,
	waitForToolState,
} from './helpers/state.js';

test.describe('arc tool regression', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('click-click arc creates 1 arc + 3 points', async ({ waffle }) => {
		await clickArc(waffle.page);
		expect(await getActiveTool(waffle.page)).toBe('arc');

		// Arc tool: center → start → end (3 clicks)
		await drawArc(waffle.page, 0, 0, 60, 0, 0, 60);

		try {
			await waitForEntityCount(waffle.page, 4, 5000); // 3 points + 1 arc
		} catch {
			await waffle.dumpState('arc-click-click-failed');
		}

		const arcs = await getEntityCountByType(waffle.page, 'Arc');
		const points = await getEntityCountByType(waffle.page, 'Point');
		expect(arcs).toBe(1);
		expect(points).toBe(3); // center, start, end
	});

	test('arc entity has type Arc', async ({ waffle }) => {
		await clickArc(waffle.page);

		await drawArc(waffle.page, 0, 0, 60, 0, 0, 60);

		try {
			await waitForEntityCount(waffle.page, 4, 5000);
		} catch {
			await waffle.dumpState('arc-type-check-failed');
		}

		const entities = await getEntities(waffle.page);
		const arcEntities = entities.filter(e => e.type === 'Arc');
		expect(arcEntities).toHaveLength(1);
		expect(arcEntities[0].type).toBe('Arc');
	});

	test('tool state transitions correct', async ({ waffle }) => {
		await clickArc(waffle.page);

		// Before any click: tool state should be idle
		const state0 = await getToolState(waffle.page);
		expect(state0).toBe('idle');

		// First click (center)
		await clickAt(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(200);
		const state1 = await getToolState(waffle.page);
		expect(state1).toBe('centerPlaced');

		// Second click (start)
		await clickAt(waffle.page, 60, 0);
		await waffle.page.waitForTimeout(200);
		const state2 = await getToolState(waffle.page);
		expect(state2).toBe('arcStartPlaced');

		// Third click (end) — completes arc, back to idle
		await clickAt(waffle.page, 0, 60);
		await waffle.page.waitForTimeout(300);
		const state3 = await getToolState(waffle.page);
		expect(state3).toBe('idle');
	});

	test('multiple arcs accumulate correctly', async ({ waffle }) => {
		// Draw a line first: 2 Points + 1 Line = 3 entities
		await drawLine(waffle.page, -100, 0, 100, 0);
		try {
			await waitForEntityCount(waffle.page, 3, 3000);
		} catch {
			await waffle.dumpState('arc-accum-line-failed');
		}

		const countAfterLine = await getEntityCount(waffle.page);
		expect(countAfterLine).toBe(3);

		// Switch to arc tool and draw an arc: 3 Points + 1 Arc = 4 more entities
		await clickArc(waffle.page);
		await drawArc(waffle.page, 0, 50, 60, 50, 0, 110);

		try {
			await waitForEntityCount(waffle.page, 7, 5000); // 3 + 4 = 7
		} catch {
			await waffle.dumpState('arc-accum-arc-failed');
		}

		const totalCount = await getEntityCount(waffle.page);
		expect(totalCount).toBe(7);
	});

	test('Escape cancels partial arc', async ({ waffle }) => {
		await clickArc(waffle.page);

		// Place center (1st click)
		await clickAt(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(200);

		const stateAfterCenter = await getToolState(waffle.page);
		expect(stateAfterCenter).toBe('centerPlaced');

		// Press Escape to cancel
		await pressKey(waffle.page, 'Escape');

		// Should be back on select tool (Escape exits tool)
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('select');

		// No arc entity should have been created
		const arcs = await getEntityCountByType(waffle.page, 'Arc');
		expect(arcs).toBe(0);
	});

	test('arc after tool switch works', async ({ waffle }) => {
		// Start with line tool (default after sketch entry)
		expect(await getActiveTool(waffle.page)).toBe('line');

		// Place first point of a line but don't finish
		await clickAt(waffle.page, -100, 0);
		await waffle.page.waitForTimeout(200);

		// Switch to arc tool mid-drawing
		await clickArc(waffle.page);
		expect(await getActiveTool(waffle.page)).toBe('arc');

		// Draw a complete arc
		await drawArc(waffle.page, 0, 0, 60, 0, 0, 60);

		try {
			await waitForEntityCount(waffle.page, 4, 5000);
		} catch {
			await waffle.dumpState('arc-tool-switch-failed');
		}

		// Arc entity should exist
		const arcs = await getEntityCountByType(waffle.page, 'Arc');
		expect(arcs).toBeGreaterThanOrEqual(1);
	});
});
