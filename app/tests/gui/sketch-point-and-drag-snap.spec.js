/**
 * Covers two sketch UX additions:
 *  (#3) the standalone Point tool drops a sketch point on click;
 *  (#2) dragging a point onto the origin pins it (WhereDragged 0,0), so it can
 *       no longer drift — previously the origin snap during drag created no
 *       constraint.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickSelect } from './helpers/toolbar.js';
import { drawLine, clickAt, dragLine } from './helpers/canvas.js';
import { getEntities, waitForEntityCount } from './helpers/state.js';
import { getConstraints } from './helpers/constraint.js';

async function positions(page) {
	return page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
}
async function sketchToOffset(page, sx, sy) {
	return page.evaluate(([x, y]) => window.__waffle.sketchToScreenOffset(x, y), [sx, sy]);
}

test.describe('sketch point tool + drag-to-origin snap', () => {
	test('point tool drops a standalone sketch point', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		await page.evaluate(() => window.__waffle.setTool('point'));
		await page.waitForTimeout(100);

		const before = (await getEntities(page)).filter((e) => e.type === 'Point').length;
		await clickAt(page, 60, 30);
		await page.waitForTimeout(200);
		await clickAt(page, -40, -50);
		await page.waitForTimeout(200);

		const after = (await getEntities(page)).filter((e) => e.type === 'Point').length;
		expect(after).toBe(before + 2);
	});

	test('dragging a point onto the origin pins it (WhereDragged 0,0)', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a line off-origin → 2 endpoints.
		await clickLine(page);
		await drawLine(page, -80, 60, 80, 60);
		await waitForEntityCount(page, 3, 5000);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		await clickSelect(page);

		// Grab one endpoint and drag it onto the origin.
		const pos = await positions(page);
		const pts = (await getEntities(page)).filter((e) => e.type === 'Point');
		const grab = pts[0];
		const from = await sketchToOffset(page, pos[grab.id].x, pos[grab.id].y);
		const origin = await sketchToOffset(page, 0, 0);
		await dragLine(page, from.x, from.y, origin.x, origin.y);
		await page.waitForTimeout(400);

		// A permanent origin pin (WhereDragged at 0,0) should now exist.
		const cons = await getConstraints(page);
		const pin = cons.find((c) => c.type === 'WhereDragged'
			&& Math.abs(c.x) < 1e-6 && Math.abs(c.y) < 1e-6 && !c._isDrag);
		expect(pin, 'origin snap on drag-release creates a WhereDragged(0,0)').toBeTruthy();

		// And the grabbed point sits at the origin.
		const after = await positions(page);
		expect(Math.hypot(after[grab.id].x, after[grab.id].y)).toBeLessThan(1e-4);
	});
});
