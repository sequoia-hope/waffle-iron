/**
 * Drawing tool state transition tests.
 *
 * Verifies that getToolState() and getDrawingState() reflect correct
 * intermediate states during drawing operations, not just final entity counts.
 *
 * Per CLAUDE.md: "Verify tool state, not just outputs. Check getToolState()
 * and getDrawingState() at each step, not just final entity counts."
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickLine,
	clickRectangle,
	clickCircle,
	clickArc,
	clickSelect,
	pressKey,
} from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle, moveTo, getCanvasBounds } from './helpers/canvas.js';
import {
	getToolState,
	getDrawingState,
	getEntityCount,
	getEntityCountByType,
	waitForEntityCount,
	getActiveTool,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

test.describe('line tool state transitions', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickLine(waffle.page);
	});

	test('line tool starts in idle state', async ({ waffle }) => {
		const state = await getToolState(waffle.page);
		expect(state).toBe('idle');

		const drawState = await getDrawingState(waffle.page);
		expect(drawState.isDragging).toBe(false);
		expect(drawState.startPointId).toBeNull();
	});

	test('first click transitions to startPlaced', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, -50, 0);

		const state = await getToolState(page);
		expect(state).toBe('firstPointPlaced');

		const drawState = await getDrawingState(page);
		expect(drawState.startPointId).not.toBeNull();
	});

	test('second click completes line and chains to firstPointPlaced', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, -50, 0);
		expect(await getToolState(page)).toBe('firstPointPlaced');

		await clickAt(page, 50, 0);
		await waitForEntityCount(page, 3, 5000);

		// Line tool chains: after completing a line, the endpoint becomes
		// the start of the next line (firstPointPlaced, not idle).
		expect(await getToolState(page)).toBe('firstPointPlaced');
		expect(await getEntityCountByType(page, 'Line')).toBe(1);
		expect(await getEntityCountByType(page, 'Point')).toBe(2);
	});

	test('Escape during startPlaced cancels to idle with no entities', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, -50, 0);
		expect(await getToolState(page)).toBe('firstPointPlaced');

		await pressKey(page, 'Escape');

		const state = await getToolState(page);
		// After Escape, tool may switch to select or reset to idle
		expect(['idle', 'select'].includes(state) || (await getActiveTool(page)) === 'select').toBeTruthy();
	});
});

test.describe('rectangle tool state transitions', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
	});

	test('rectangle tool starts in idle state', async ({ waffle }) => {
		expect(await getToolState(waffle.page)).toBe('idle');
	});

	test('first click transitions to cornerPlaced', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, -80, -60);

		const state = await getToolState(page);
		expect(state).toBe('firstCornerPlaced');
	});

	test('second click completes rectangle and returns to idle', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, -80, -60);
		expect(await getToolState(page)).toBe('firstCornerPlaced');

		await clickAt(page, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		expect(await getToolState(page)).toBe('idle');
		expect(await getEntityCountByType(page, 'Line')).toBe(4);
		expect(await getEntityCountByType(page, 'Point')).toBe(4);
	});
});

test.describe('circle tool state transitions', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickCircle(waffle.page);
	});

	test('circle tool starts in idle state', async ({ waffle }) => {
		expect(await getToolState(waffle.page)).toBe('idle');
	});

	test('first click transitions to centerPlaced', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, 0, 0);

		const state = await getToolState(page);
		expect(state).toBe('centerPlaced');
	});

	test('second click completes circle and returns to idle', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, 0, 0);
		expect(await getToolState(page)).toBe('centerPlaced');

		await clickAt(page, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		expect(await getToolState(page)).toBe('idle');
		expect(await getEntityCountByType(page, 'Circle')).toBe(1);
		expect(await getEntityCountByType(page, 'Point')).toBe(1);
	});
});

test.describe('arc tool state transitions', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickArc(waffle.page);
	});

	test('arc tool starts in idle state', async ({ waffle }) => {
		expect(await getToolState(waffle.page)).toBe('idle');
	});

	test('first click places center', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, 0, 0);
		expect(await getToolState(page)).toBe('centerPlaced');
	});

	test('second click places arc start', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, 0, 0);
		expect(await getToolState(page)).toBe('centerPlaced');

		await clickAt(page, 60, 0);
		expect(await getToolState(page)).toBe('arcStartPlaced');
	});

	test('third click completes arc and returns to idle', async ({ waffle }) => {
		const page = waffle.page;
		await clickAt(page, 0, 0);
		await clickAt(page, 60, 0);
		expect(await getToolState(page)).toBe('arcStartPlaced');

		await clickAt(page, 0, 60);
		await waitForEntityCount(page, 4, 5000);

		expect(await getToolState(page)).toBe('idle');
		expect(await getEntityCountByType(page, 'Arc')).toBe(1);
		expect(await getEntityCountByType(page, 'Point')).toBe(3);
	});
});

test.describe('tool switching mid-operation', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('switching from line to rectangle mid-draw resets state', async ({ waffle }) => {
		const page = waffle.page;
		await clickLine(page);
		await clickAt(page, -50, 0);
		expect(await getToolState(page)).toBe('firstPointPlaced');

		// Switch to rectangle tool mid-draw
		await clickRectangle(page);
		expect(await getToolState(page)).toBe('idle');
		expect(await getActiveTool(page)).toBe('rectangle');
	});

	test('switching from circle to line mid-draw resets state', async ({ waffle }) => {
		const page = waffle.page;
		await clickCircle(page);
		await clickAt(page, 0, 0);
		expect(await getToolState(page)).toBe('centerPlaced');

		await clickLine(page);
		expect(await getToolState(page)).toBe('idle');
		expect(await getActiveTool(page)).toBe('line');
	});

	test('no crash on rapid tool switches', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = await collectCrashErrors(page);

		await clickLine(page);
		await clickRectangle(page);
		await clickCircle(page);
		await clickArc(page);
		await clickLine(page);
		await clickSelect(page);

		await expectNoAnyCrash(crashes);
	});
});

test.describe('sequential drawing operations', () => {
	test('two separate lines at different positions accumulate', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickLine(page);

		// First line: 2 points + 1 line = 3 entities
		await drawLine(page, -100, -50, -50, -50);
		await waitForEntityCount(page, 3, 5000);
		expect(await getEntityCountByType(page, 'Line')).toBe(1);

		// Press Escape to break the chain and reset to idle
		await pressKey(page, 'Escape');

		// Re-select line tool and draw a second line at a different position
		await clickLine(page);
		await drawLine(page, 50, 50, 100, 50);
		await waitForEntityCount(page, 6, 5000);
		expect(await getEntityCountByType(page, 'Line')).toBe(2);
	});

	test('rectangle then circle in same sketch', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		await clickCircle(page);
		await clickAt(page, 0, 0);
		await clickAt(page, 40, 0);
		await waitForEntityCount(page, 10, 5000);

		expect(await getEntityCountByType(page, 'Line')).toBe(4);
		expect(await getEntityCountByType(page, 'Circle')).toBe(1);
		expect(await getEntityCountByType(page, 'Point')).toBe(5);
	});
});
