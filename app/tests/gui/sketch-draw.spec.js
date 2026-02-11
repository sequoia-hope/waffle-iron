/**
 * Drawing via mouse on canvas — line, rectangle, circle tools.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickRectangle, clickCircle, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle, drawCircle } from './helpers/canvas.js';
import { getActiveTool, getEntityCount, getEntityCountByType, getEntities, waitForEntityCount } from './helpers/state.js';

test.describe('sketch drawing via GUI', () => {
	test.beforeEach(async ({ waffle }) => {
		// Enter sketch mode before each drawing test
		await clickSketch(waffle.page);
	});

	test('clicking Line button sets line tool', async ({ waffle }) => {
		// Sketch mode starts with line tool by default, switch away first
		await pressKey(waffle.page, 'Escape');
		const tool1 = await getActiveTool(waffle.page);
		expect(tool1).toBe('select');

		await clickLine(waffle.page);
		const tool2 = await getActiveTool(waffle.page);
		expect(tool2).toBe('line');
	});

	test('draw line with two clicks creates 2 Points + 1 Line', async ({ waffle }) => {
		// Should already be in line tool from sketch entry
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('line');

		await drawLine(waffle.page, -100, 0, 100, 0);

		// Wait for entities to appear
		try {
			await waitForEntityCount(waffle.page, 3, 3000);
		} catch {
			// If entities didn't appear, dump state for debugging
			await waffle.dumpState('draw-line-failed');
		}

		const points = await getEntityCountByType(waffle.page, 'Point');
		const lines = await getEntityCountByType(waffle.page, 'Line');
		expect(points).toBeGreaterThanOrEqual(2);
		expect(lines).toBeGreaterThanOrEqual(1);
	});

	test('draw rectangle creates 4 Points + 4 Lines', async ({ waffle }) => {
		await clickRectangle(waffle.page);
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('rectangle');

		await drawRectangle(waffle.page, -80, -60, 80, 60);

		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('draw-rectangle-failed');
		}

		const points = await getEntityCountByType(waffle.page, 'Point');
		const lines = await getEntityCountByType(waffle.page, 'Line');
		expect(points).toBe(4);
		expect(lines).toBe(4);
	});

	test('draw circle creates center Point + Circle', async ({ waffle }) => {
		await clickCircle(waffle.page);
		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('circle');

		await drawCircle(waffle.page, 0, 0, 60, 0);

		try {
			await waitForEntityCount(waffle.page, 2, 3000);
		} catch {
			await waffle.dumpState('draw-circle-failed');
		}

		const circles = await getEntityCountByType(waffle.page, 'Circle');
		expect(circles).toBeGreaterThanOrEqual(1);
	});

	test('L key switches to line tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'Escape'); // go to select first
		await pressKey(waffle.page, 'l');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('line');
	});

	test('R key switches to rectangle tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'r');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('rectangle');
	});

	test('C key switches to circle tool', async ({ waffle }) => {
		await pressKey(waffle.page, 'c');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('circle');
	});

	test('Escape switches from tool to select', async ({ waffle }) => {
		const tool1 = await getActiveTool(waffle.page);
		expect(tool1).toBe('line'); // default after sketch entry

		await pressKey(waffle.page, 'Escape');
		const tool2 = await getActiveTool(waffle.page);
		expect(tool2).toBe('select');
	});
});
