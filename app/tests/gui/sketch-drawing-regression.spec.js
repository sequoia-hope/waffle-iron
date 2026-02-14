/**
 * Sketch drawing regression tests — strict canary.
 *
 * These are the simplest, most direct drawing tests possible.
 * If ANY of these fail, drawing is broken. No try/catch, no error swallowing.
 * Run before every commit that touches sketch code.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickRectangle, clickCircle } from './helpers/toolbar.js';
import { drawLine, drawRectangle, drawCircle, dragLine, dragRectangle, dragCircle } from './helpers/canvas.js';
import { getEntityCountByType, waitForEntityCount } from './helpers/state.js';

test.describe('sketch drawing regression — click-click', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('click-click line creates exactly 1 line', async ({ waffle }) => {
		const page = waffle.page;

		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const points = await getEntityCountByType(page, 'Point');
		const lines = await getEntityCountByType(page, 'Line');
		expect(points).toBe(2);
		expect(lines).toBe(1);
	});

	test('click-click rectangle creates 4 lines + 4 points', async ({ waffle }) => {
		const page = waffle.page;

		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		const points = await getEntityCountByType(page, 'Point');
		const lines = await getEntityCountByType(page, 'Line');
		expect(points).toBe(4);
		expect(lines).toBe(4);
	});

	test('click-click circle creates 1 circle + 1 point', async ({ waffle }) => {
		const page = waffle.page;

		await clickCircle(page);
		await drawCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		const points = await getEntityCountByType(page, 'Point');
		const circles = await getEntityCountByType(page, 'Circle');
		expect(points).toBe(1);
		expect(circles).toBe(1);
	});
});

test.describe('sketch drawing regression — click-drag', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('click-drag line creates exactly 1 line', async ({ waffle }) => {
		const page = waffle.page;

		await dragLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const points = await getEntityCountByType(page, 'Point');
		const lines = await getEntityCountByType(page, 'Line');
		expect(points).toBe(2);
		expect(lines).toBe(1);
	});

	test('click-drag rectangle creates 4 lines + 4 points', async ({ waffle }) => {
		const page = waffle.page;

		await clickRectangle(page);
		await dragRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		const points = await getEntityCountByType(page, 'Point');
		const lines = await getEntityCountByType(page, 'Line');
		expect(points).toBe(4);
		expect(lines).toBe(4);
	});

	test('click-drag circle creates 1 circle + 1 point', async ({ waffle }) => {
		const page = waffle.page;

		await clickCircle(page);
		await dragCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		const points = await getEntityCountByType(page, 'Point');
		const circles = await getEntityCountByType(page, 'Circle');
		expect(points).toBe(1);
		expect(circles).toBe(1);
	});
});
