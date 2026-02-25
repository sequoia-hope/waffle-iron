/**
 * Arc tool interaction tests — drag mode limitations, cancellation,
 * and entity verification.
 *
 * The arc tool uses a 3-click state machine: center → start → end.
 * Drag mode (mousedown + move + mouseup) does NOT complete the arc
 * because the tool requires discrete click events for each point.
 * Drag-creation tests are marked test.fixme() to document this.
 *
 * Click-click arc is covered in arc-regression.spec.js.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickArc, pressKey } from './helpers/toolbar.js';
import { clickAt, drawArc, dragArc } from './helpers/canvas.js';
import {
	getActiveTool,
	getEntityCount,
	getEntityCountByType,
	waitForEntityCount,
	getToolState,
} from './helpers/state.js';

test.describe('arc drag mode (not yet supported)', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickArc(waffle.page);
	});

	test.fixme('drag arc creates 1 arc + 3 points', async ({ waffle }) => {
		// Arc tool requires 3 discrete clicks (center, start, end).
		// dragArc clicks center then drags start→end, but the drag
		// only registers as one event, leaving the arc incomplete.
		await dragArc(waffle.page, 0, 0, 60, 0, 0, 60);

		await waitForEntityCount(waffle.page, 4, 5000); // 3 points + 1 arc

		const arcs = await getEntityCountByType(waffle.page, 'Arc');
		const points = await getEntityCountByType(waffle.page, 'Point');
		expect(arcs).toBe(1);
		expect(points).toBe(3);
	});

	test.fixme('drag arc returns to idle state after completion', async ({ waffle }) => {
		// Drag mode doesn't complete the arc, so state remains arcStartPlaced.
		await dragArc(waffle.page, 0, 0, 60, 0, 0, 60);

		await waitForEntityCount(waffle.page, 4, 5000);

		const state = await getToolState(waffle.page);
		expect(state).toBe('idle');
	});

	test.fixme('two consecutive drag arcs accumulate entities', async ({ waffle }) => {
		// Requires drag mode to work for arcs.
		await dragArc(waffle.page, -50, 0, -50, 40, -50, -40);
		await waitForEntityCount(waffle.page, 4, 5000);

		await dragArc(waffle.page, 50, 0, 50, 40, 50, -40);
		await waitForEntityCount(waffle.page, 8, 5000);

		const arcs = await getEntityCountByType(waffle.page, 'Arc');
		expect(arcs).toBe(2);
	});
});

test.describe('arc cancellation', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickArc(waffle.page);
	});

	test('Escape after center click cancels without creating arc', async ({ waffle }) => {
		// Place center (first click)
		await clickAt(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(200);

		const stateAfterCenter = await getToolState(waffle.page);
		expect(stateAfterCenter).toBe('centerPlaced');

		// Press Escape to cancel
		await pressKey(waffle.page, 'Escape');

		// Should switch to select tool
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('select');

		// No arc should exist
		const arcs = await getEntityCountByType(waffle.page, 'Arc');
		expect(arcs).toBe(0);
	});

	test('Escape after center + start cancels without arc entity', async ({ waffle }) => {
		// Place center
		await clickAt(waffle.page, 0, 0);
		await waffle.page.waitForTimeout(200);

		// Place start point
		await clickAt(waffle.page, 60, 0);
		await waffle.page.waitForTimeout(200);

		const state = await getToolState(waffle.page);
		expect(state).toBe('arcStartPlaced');

		// Press Escape to cancel
		await pressKey(waffle.page, 'Escape');

		// No arc should exist
		const arcs = await getEntityCountByType(waffle.page, 'Arc');
		expect(arcs).toBe(0);
	});
});

test.describe('arc click-click verification', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickArc(waffle.page);
	});

	test('click-click arc creates correct entity counts', async ({ waffle }) => {
		await drawArc(waffle.page, 0, 0, 60, 0, 0, 60);

		try {
			await waitForEntityCount(waffle.page, 4, 5000);
		} catch {
			await waffle.dumpState('click-arc-count-failed');
		}

		const clickClickCount = await getEntityCount(waffle.page);
		const clickClickArcs = await getEntityCountByType(waffle.page, 'Arc');
		const clickClickPoints = await getEntityCountByType(waffle.page, 'Point');

		expect(clickClickArcs).toBe(1);
		expect(clickClickPoints).toBe(3);
		expect(clickClickCount).toBe(4);
	});

	test('arc tool remains active after completing an arc', async ({ waffle }) => {
		await drawArc(waffle.page, 0, 0, 60, 0, 0, 60);

		try {
			await waitForEntityCount(waffle.page, 4, 5000);
		} catch {
			await waffle.dumpState('arc-tool-persist-failed');
		}

		// Tool should remain as arc (ready for next arc)
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('arc');

		// State should be idle (ready for next arc's center click)
		const state = await getToolState(waffle.page);
		expect(state).toBe('idle');
	});
});
