/**
 * Constraint badges are interactive: click to select, Delete to remove, drag to
 * reposition. (Dimensional value editing lives on the HTML dimension labels.)
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickSelect } from './helpers/toolbar.js';
import { drawRectangle, clickAt, dragLine } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';
import { getConstraintCount } from './helpers/constraint.js';

async function sketchToOffset(page, sx, sy) {
	return page.evaluate(([x, y]) => window.__waffle.sketchToScreenOffset(x, y), [sx, sy]);
}
async function badges(page) {
	return page.evaluate(() => window.__waffle.getConstraintBadges());
}
/** Screen offset of the first badge (its computed sketch position). */
async function firstBadgeOffset(page) {
	const list = await badges(page);
	expect(list.length, 'at least one badge').toBeGreaterThan(0);
	return sketchToOffset(page, list[0].sx, list[0].sy);
}

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

		// Click the first badge at its actual computed position.
		const badge = await firstBadgeOffset(page);
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

		const badge = await firstBadgeOffset(page);
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
