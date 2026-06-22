/**
 * Constraint badges are interactive: click to select, Delete to remove, drag to
 * reposition. (Dimensional value editing lives on the HTML dimension labels.)
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickSelect } from './helpers/toolbar.js';
import { drawRectangle, clickAt, dragLine } from './helpers/canvas.js';
import { getEntities, waitForEntityCount } from './helpers/state.js';
import { getConstraintCount } from './helpers/constraint.js';

async function positions(page) {
	return page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
}
async function sketchToOffset(page, sx, sy) {
	return page.evaluate(([x, y]) => window.__waffle.sketchToScreenOffset(x, y), [sx, sy]);
}
async function rectBBox(page) {
	const pos = await positions(page);
	const pts = (await getEntities(page)).filter((e) => e.type === 'Point').map((e) => e.id);
	const xs = pts.map((id) => pos[id].x), ys = pts.map((id) => pos[id].y);
	return {
		minX: Math.min(...xs), maxX: Math.max(...xs),
		minY: Math.min(...ys), maxY: Math.max(...ys),
		cx: (Math.min(...xs) + Math.max(...xs)) / 2,
		cy: (Math.min(...ys) + Math.max(...ys)) / 2,
	};
}

const V_OFFSET = 0.00015; // matches constraintBadges.js

test.describe('constraint badge interaction', () => {
	test('click selects a badge, Delete removes the constraint', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -120, -120, 120, 120);
		await waitForEntityCount(page, 8, 5000);
		await page.waitForTimeout(400);

		// Rectangle auto-applies 4 H/V constraints → 4 badges.
		expect(await getConstraintCount(page)).toBe(4);

		await clickSelect(page);

		// The bottom edge's 'H' badge sits at (cx, minY + V_OFFSET).
		const bb = await rectBBox(page);
		const badge = await sketchToOffset(page, bb.cx, bb.minY + V_OFFSET);
		await clickAt(page, badge.x, badge.y);
		await page.waitForTimeout(150);

		// A constraint badge is now selected.
		const sel = await page.evaluate(() => window.__waffle.getSelectedConstraintIndex());
		expect(sel, 'clicking a badge selects its constraint').not.toBeNull();

		// Delete removes it.
		await page.keyboard.press('Delete');
		await page.waitForTimeout(300);
		expect(await getConstraintCount(page)).toBe(3);
		expect(await page.evaluate(() => window.__waffle.getSelectedConstraintIndex())).toBeNull();
	});

	test('dragging a badge stores a display offset', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -120, -120, 120, 120);
		await waitForEntityCount(page, 8, 5000);
		await page.waitForTimeout(400);
		await clickSelect(page);

		const bb = await rectBBox(page);
		const badge = await sketchToOffset(page, bb.cx, bb.minY + V_OFFSET);
		// Drag the badge sideways.
		await dragLine(page, badge.x, badge.y, badge.x + 40, badge.y + 10);
		await page.waitForTimeout(300);

		const offsets = await page.evaluate(() => window.__waffle.getConstraintBadgeOffsets());
		const keys = Object.keys(offsets);
		expect(keys.length, 'a badge offset was recorded').toBeGreaterThan(0);
		const moved = keys.some((k) => Math.hypot(offsets[k].dx, offsets[k].dy) > 1e-5);
		expect(moved, 'the dragged badge has a non-zero offset').toBe(true);
	});
});
