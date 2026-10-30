/**
 * A shape cancelled mid-construction leaves nothing behind (user report
 * 2026-10-30: "dropping a rectangle or other sketch feature's first point
 * and then cancelling is leaving the first point behind").
 *
 * Each multi-click tool mints its first point on the first click; a cancel
 * (Escape, or switching tools) must take that point away again, together with
 * any snap constraint that named it, and leave the tool idle. Finished
 * geometry is NOT cancelled: an open polyline ended with Escape keeps its
 * segments. Real pointer events, click-click and click-drag.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickTool, clickCenterRectangle } from './helpers/toolbar.js';
import { clickAt, getCanvasBounds } from './helpers/canvas.js';
import { getEntityCount, getToolState, getEntityCountByType } from './helpers/state.js';
import { getConstraintCount } from './helpers/constraint.js';

/** Arm `tool` in a fresh sketch on the Front plane. */
async function armTool(page, tool) {
	await clickSketch(page);
	if (tool === 'rectangle-center') await clickCenterRectangle(page);
	else await clickTool(page, tool);
}

async function expectNothingLeft(page, what) {
	await expect.poll(() => getEntityCount(page), { message: `${what}: entities` }).toBe(0);
	expect(await getConstraintCount(page), `${what}: constraints`).toBe(0);
}

const TOOLS = ['line', 'rectangle', 'rectangle-center', 'circle', 'arc', 'slot', 'polyline'];

test.describe('cancelling an unfinished sketch shape', () => {
	for (const tool of TOOLS) {
		test(`${tool}: click once, Escape → nothing left`, async ({ waffle }) => {
			const page = waffle.page;
			await armTool(page, tool);
			await clickAt(page, -60, -40);
			await expect.poll(() => getEntityCount(page), { message: 'first click minted a point' }).toBeGreaterThan(0);
			expect(await getToolState(page)).not.toBe('idle');

			await page.keyboard.press('Escape');
			await expectNothingLeft(page, `${tool} Escape`);
			expect(await getToolState(page)).toBe('idle');
		});
	}

	test('line: click once, switch tool → nothing left', async ({ waffle }) => {
		const page = waffle.page;
		await armTool(page, 'line');
		await clickAt(page, -60, -40);
		await expect.poll(() => getEntityCount(page)).toBeGreaterThan(0);
		await clickTool(page, 'circle');
		await expectNothingLeft(page, 'line → circle');
		expect(await getToolState(page)).toBe('idle');
	});

	test('rectangle: press, drag part-way, Escape, release → nothing left', async ({ waffle }) => {
		const page = waffle.page;
		await armTool(page, 'rectangle');
		const b = await getCanvasBounds(page);
		if (!b) throw new Error('Canvas not visible');
		await page.mouse.move(b.centerX - 60, b.centerY - 40);
		await page.mouse.down();
		for (let i = 1; i <= 6; i++) await page.mouse.move(b.centerX - 60 + i * 10, b.centerY - 40 + i * 6);
		await expect.poll(() => getEntityCount(page)).toBeGreaterThan(0);
		await page.keyboard.press('Escape');
		await page.mouse.up();
		await expectNothingLeft(page, 'rectangle drag + Escape');
	});

	test('the first click on an existing sketch point does not delete that point', async ({ waffle }) => {
		const page = waffle.page;
		await armTool(page, 'line');
		await clickAt(page, -60, -40);
		await clickAt(page, 60, -40);
		await page.keyboard.press('Escape');
		await expect.poll(() => getEntityCountByType(page, 'Line')).toBe(1);
		const before = await getEntityCount(page);
		// Start a new line ON the existing endpoint, then cancel it.
		await clickTool(page, 'line');
		await clickAt(page, 60, -40);
		await page.keyboard.press('Escape');
		await expect.poll(() => getEntityCount(page)).toBe(before);
		expect(await getEntityCountByType(page, 'Line')).toBe(1);
	});

	test('polyline: Escape after two segments keeps them', async ({ waffle }) => {
		const page = waffle.page;
		await armTool(page, 'polyline');
		await clickAt(page, -80, 20);
		await clickAt(page, 0, -40);
		await clickAt(page, 80, 20);
		await page.keyboard.press('Escape');
		await expect.poll(() => getEntityCountByType(page, 'Line')).toBe(2);
		expect(await getEntityCountByType(page, 'Point')).toBe(3);
	});
});
