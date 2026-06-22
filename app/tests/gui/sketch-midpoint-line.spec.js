/**
 * Regression: a line drawn between the midpoints of two opposite rectangle
 * sides must (B) be pinned to those midpoints via Midpoint constraints, and
 * (A) be draggable by its body to relocate the whole figure.
 *
 * Bug report: drawing such a "center line" created no Midpoint constraints
 * (dragging the square left the line behind) and the line could not be grabbed
 * by its middle to drag at all.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickLine, clickSelect } from './helpers/toolbar.js';
import { drawRectangle, moveTo, clickAt, dragLine } from './helpers/canvas.js';
import { getEntities, waitForEntityCount } from './helpers/state.js';
import { getConstraintCountByType } from './helpers/constraint.js';

/** Read solved sketch positions as a plain {id: {x,y}} object. */
async function positions(page) {
	return page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
}
/** Map sketch coords → canvas-center-relative screen offset. */
async function sketchToOffset(page, sx, sy) {
	return page.evaluate(([x, y]) => window.__waffle.sketchToScreenOffset(x, y), [sx, sy]);
}

function bboxCenter(pos, pointIds) {
	const xs = pointIds.map((id) => pos[id].x);
	const ys = pointIds.map((id) => pos[id].y);
	return {
		cx: (Math.min(...xs) + Math.max(...xs)) / 2,
		cy: (Math.min(...ys) + Math.max(...ys)) / 2,
		minX: Math.min(...xs), maxX: Math.max(...xs),
		minY: Math.min(...ys), maxY: Math.max(...ys),
	};
}

test.describe('sketch midpoint center line', () => {
	test('line between opposite side midpoints is pinned and body-draggable', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		// Draw a rectangle (4 points + 4 lines).
		await clickRectangle(page);
		await drawRectangle(page, -120, -120, 120, 120);
		await waitForEntityCount(page, 8, 5000);
		await page.waitForTimeout(400);

		// Rectangle corner ids + sketch-space bbox.
		const rectPos = await positions(page);
		const rectPts = (await getEntities(page)).filter((e) => e.type === 'Point').map((e) => e.id);
		const bb = bboxCenter(rectPos, rectPts);

		// Bottom- and top-edge midpoints in sketch coords → screen offsets.
		const bot = await sketchToOffset(page, bb.cx, bb.minY);
		const top = await sketchToOffset(page, bb.cx, bb.maxY);
		expect(bot, 'bottom midpoint maps to screen').not.toBeNull();
		expect(top, 'top midpoint maps to screen').not.toBeNull();

		// Draw a line snapping to each side midpoint (click-click).
		await clickLine(page);
		await moveTo(page, bot.x, bot.y);
		await page.waitForTimeout(150);
		await clickAt(page, bot.x, bot.y);
		await moveTo(page, top.x, top.y);
		await page.waitForTimeout(150);
		await clickAt(page, top.x, top.y);
		await page.waitForTimeout(200);
		await page.keyboard.press('Escape'); // finish line chaining
		await page.waitForTimeout(300);

		// (B) Two Midpoint constraints were created — the line is pinned.
		expect(await getConstraintCountByType(page, 'Midpoint')).toBe(2);

		// (A) Grab the line by its middle and drag it; the whole figure should move.
		await clickSelect(page);
		const beforePos = await positions(page);
		const beforeCenter = bboxCenter(beforePos, rectPts);

		const mid = await sketchToOffset(page, bb.cx, bb.cy);
		await dragLine(page, mid.x, mid.y, mid.x + 50, mid.y + 50);
		await page.waitForTimeout(400);

		const afterPos = await positions(page);
		const afterCenter = bboxCenter(afterPos, rectPts);

		// The rectangle moved (the line was draggable and pulled the square along).
		const moved = Math.hypot(afterCenter.cx - beforeCenter.cx, afterCenter.cy - beforeCenter.cy);
		expect(moved, 'dragging the center line relocates the figure').toBeGreaterThan(1e-3);

		// The Midpoint pinning still holds: line endpoints sit on the side midpoints.
		// The center line is the one whose endpoints are NOT rectangle corners.
		const allLines = (await getEntities(page)).filter((e) => e.type === 'Line');
		const center = allLines.find((l) => !rectPts.includes(l.start_id) && !rectPts.includes(l.end_id));
		expect(center, 'center line exists').toBeTruthy();
		const sp = afterPos[center.start_id];
		const ep = afterPos[center.end_id];
		// Each endpoint x should equal the rectangle mid-x (pinned to a vertical-side midpoint).
		expect(Math.abs(sp.x - afterCenter.cx)).toBeLessThan(0.05);
		expect(Math.abs(ep.x - afterCenter.cx)).toBeLessThan(0.05);
	});
});
