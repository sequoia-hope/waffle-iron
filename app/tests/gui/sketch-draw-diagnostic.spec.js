/**
 * Sketch drawing diagnostic tests — full state machine instrumentation.
 *
 * These tests verify not just entity counts but the entire drawing pipeline:
 * 1. Events reaching the handler (tool event log)
 * 2. Tool state transitions (idle → firstPointPlaced → finalized)
 * 3. Entities created with correct types and references
 * 4. Both click-click AND click-drag drawing modes
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickRectangle, clickCircle } from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle, drawCircle, dragLine, dragRectangle, dragCircle } from './helpers/canvas.js';
import {
	getActiveTool, getEntityCount, getEntityCountByType, getEntities,
	waitForEntityCount, getToolState, getDrawingState, getToolEventLog,
	clearToolEventLog, waitForToolState, waitForToolEvent
} from './helpers/state.js';

test.describe('click-click line drawing diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('first click transitions tool state to firstPointPlaced', async ({ waffle }) => {
		const page = waffle.page;
		const tool = await getActiveTool(page);
		expect(tool).toBe('line');

		await clearToolEventLog(page);
		await clickAt(page, -100, 0);

		const state = await getToolState(page);
		expect(state).toBe('firstPointPlaced');

		const entityCount = await getEntityCount(page);
		expect(entityCount).toBe(1); // start point only
	});

	test('second click creates line and chains (stays firstPointPlaced)', async ({ waffle }) => {
		const page = waffle.page;

		await drawLine(page, -100, 0, 100, 0);

		await waitForEntityCount(page, 3, 5000);

		const state = await getToolState(page);
		expect(state).toBe('firstPointPlaced'); // chaining: end becomes next start

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');
		expect(points.length).toBe(2);
		expect(lines.length).toBe(1);

		// Verify line references valid point IDs
		const pointIds = new Set(points.map(p => p.id));
		expect(pointIds.has(lines[0].start_id)).toBe(true);
		expect(pointIds.has(lines[0].end_id)).toBe(true);
	});

	test('event log shows both pointerdown events reaching handler', async ({ waffle }) => {
		const page = waffle.page;

		await clearToolEventLog(page);
		await drawLine(page, -100, 0, 100, 0);

		const log = await getToolEventLog(page);
		const pointerdowns = log.filter(e => e.event === 'pointerdown' && e.tool === 'line');
		expect(pointerdowns.length).toBeGreaterThanOrEqual(2);
	});
});

test.describe('click-drag line drawing diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('drag exceeding threshold sets isDragging', async ({ waffle }) => {
		const page = waffle.page;
		const tool = await getActiveTool(page);
		expect(tool).toBe('line');

		// Start the drag manually: mousedown then move 100px
		const { getCanvasBounds } = await import('./helpers/canvas.js');
		const bounds = await getCanvasBounds(page);
		const sx = bounds.centerX - 100, sy = bounds.centerY;
		const ex = bounds.centerX + 100, ey = bounds.centerY;

		await page.mouse.move(sx, sy);
		await page.mouse.down();
		// Move enough to exceed drag threshold
		for (let i = 1; i <= 10; i++) {
			const t = i / 10;
			await page.mouse.move(sx + (ex - sx) * t, sy + (ey - sy) * t);
		}

		const drawingState = await getDrawingState(page);
		expect(drawingState.isDragging).toBe(true);
		expect(drawingState.toolState).toBe('firstPointPlaced');

		await page.mouse.up();
		await page.waitForTimeout(150);
	});

	test('drag release creates line', async ({ waffle }) => {
		const page = waffle.page;

		await dragLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');
		expect(points.length).toBe(2);
		expect(lines.length).toBe(1);
	});

	test('drag does NOT chain (resets to idle)', async ({ waffle }) => {
		const page = waffle.page;

		await dragLine(page, -100, 0, 100, 0);

		const state = await getToolState(page);
		expect(state).toBe('idle');
	});
});

test.describe('click-click rectangle diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
	});

	test('two clicks create 4 points + 4 lines', async ({ waffle }) => {
		const page = waffle.page;

		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');
		expect(points.length).toBe(4);
		expect(lines.length).toBe(4);

		// Verify tool returns to idle after rectangle
		const state = await getToolState(page);
		expect(state).toBe('idle');
	});
});

test.describe('click-drag rectangle diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
	});

	test('drag creates 4 points + 4 lines', async ({ waffle }) => {
		const page = waffle.page;

		await dragRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');
		expect(points.length).toBe(4);
		expect(lines.length).toBe(4);

		const state = await getToolState(page);
		expect(state).toBe('idle');
	});
});

test.describe('click-click circle diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickCircle(waffle.page);
	});

	test('two clicks create center point + circle', async ({ waffle }) => {
		const page = waffle.page;

		await drawCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const circles = entities.filter(e => e.type === 'Circle');
		expect(points.length).toBe(1);
		expect(circles.length).toBe(1);

		// Verify circle references center point
		expect(circles[0].center_id).toBe(points[0].id);
		expect(circles[0].radius).toBeGreaterThan(0);

		const state = await getToolState(page);
		expect(state).toBe('idle');
	});
});

test.describe('click-drag circle diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickCircle(waffle.page);
	});

	test('drag creates center point + circle', async ({ waffle }) => {
		const page = waffle.page;

		await dragCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const circles = entities.filter(e => e.type === 'Circle');
		expect(points.length).toBe(1);
		expect(circles.length).toBe(1);

		const state = await getToolState(page);
		expect(state).toBe('idle');
	});
});

test.describe('event pipeline verification', () => {
	test('pointer events reach handler in sketch mode', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		await clearToolEventLog(page);
		await clickAt(page, 50, 50);

		const log = await getToolEventLog(page);
		const hasPointerdown = log.some(e => e.event === 'pointerdown');
		expect(hasPointerdown).toBe(true);
	});

	test('coordinate conversion produces reasonable values', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		await clearToolEventLog(page);
		// Click near canvas center — should give coordinates near 0,0
		await clickAt(page, 0, 0);

		const log = await getToolEventLog(page);
		const downs = log.filter(e => e.event === 'pointerdown');
		expect(downs.length).toBeGreaterThanOrEqual(1);

		// Click at an offset — should give nonzero coordinates
		await clearToolEventLog(page);
		await clickAt(page, 100, 100);

		const log2 = await getToolEventLog(page);
		const downs2 = log2.filter(e => e.event === 'pointerdown');
		expect(downs2.length).toBeGreaterThanOrEqual(1);
		// The sketch coordinates should differ from the center click
		const entry = downs2[0];
		expect(Math.abs(entry.x) + Math.abs(entry.y)).toBeGreaterThan(0);
	});
});
