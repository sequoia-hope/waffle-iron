/**
 * Right-click constraint menu → engine wire format.
 *
 * Q1 (REVIEW.md §3.2-1): the menu's "Symmetric" used to emit `entity_c` where
 * waffle-types' `SketchConstraint::Symmetric` wants `symmetry_line`. serde
 * rejected the whole SolveSketch message, and because every solve resends the
 * FULL constraint list, every later solve in the session failed too. The
 * oracle here is geometry: the points end up mirrored about the line, and a
 * second constraint applied afterwards still moves the sketch.
 *
 * Q3 (REVIEW.md §3.2-3): "Distance" on a point + line applied 1.0 m outright.
 * It must apply the CURRENT measured distance, so geometry does not move.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, pressKey } from './helpers/toolbar.js';
import { drawLine, getCanvasBounds } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';
import { setSketchSelection, getConstraints, waitForConstraintCount } from './helpers/constraint.js';

/** Draw line A (nearly vertical) then line B (slanted, crossing it). */
async function twoLines(page) {
	await clickSketch(page);
	await drawLine(page, -10, -110, 25, 110);
	await waitForEntityCount(page, 3, 5000);
	await pressKey(page, 'Escape');
	await pressKey(page, 'l');
	await drawLine(page, -140, -40, 70, 35);
	await waitForEntityCount(page, 6, 5000);
	await clickSelect(page);
	const entities = await page.evaluate(() => window.__waffle.getEntities());
	const lines = entities.filter((e) => e.type === 'Line');
	expect(lines.length).toBe(2);
	return { a: lines[0], b: lines[1] };
}

async function positions(page) {
	return page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
}

/** Select `ids`, right-click empty canvas, click the menu item `label`. */
async function applyFromMenu(page, ids, label) {
	await setSketchSelection(page, ids);
	const bounds = await getCanvasBounds(page);
	await page.mouse.click(bounds.centerX + 200, bounds.centerY - 150, { button: 'right' });
	const item = page.locator('.constraint-menu').getByRole('menuitem', { name: label, exact: true });
	await expect(item).toBeVisible({ timeout: 3000 });
	await item.click();
}

/** Reflect p about the line through s→e. */
function mirror(p, s, e) {
	const dx = e.x - s.x, dy = e.y - s.y;
	const t = ((p.x - s.x) * dx + (p.y - s.y) * dy) / (dx * dx + dy * dy);
	const fx = s.x + t * dx, fy = s.y + t * dy;
	return { x: 2 * fx - p.x, y: 2 * fy - p.y };
}

test.describe('right-click constraint menu reaches the solver', () => {
	test('Symmetric mirrors two points about a line, and the next constraint still solves', async ({ waffle }) => {
		const page = waffle.page;
		const { a, b } = await twoLines(page);
		const before = await positions(page);
		const tol = 1e-6;

		// Not symmetric to start with — otherwise the check below proves nothing.
		const m0 = mirror(before[b.start_id], before[a.start_id], before[a.end_id]);
		expect(Math.hypot(m0.x - before[b.end_id].x, m0.y - before[b.end_id].y)).toBeGreaterThan(1e-3);

		await applyFromMenu(page, [a.id, b.start_id, b.end_id], 'Symmetric');
		await waitForConstraintCount(page, 1, 3000);
		const sym = (await getConstraints(page)).find((c) => c.type === 'Symmetric');
		expect(sym, 'a Symmetric constraint was stored').toBeTruthy();

		await expect.poll(async () => {
			const p = await positions(page);
			const m = mirror(p[b.start_id], p[a.start_id], p[a.end_id]);
			return Math.hypot(m.x - p[b.end_id].x, m.y - p[b.end_id].y);
		}, { message: 'points mirrored about line A after the Symmetric solve', timeout: 5000 }).toBeLessThan(tol);

		// A second constraint: Vertical on line A. If the Symmetric payload
		// still poisoned the list, this solve fails and A stays slanted.
		await applyFromMenu(page, [a.id], 'Vertical');
		await waitForConstraintCount(page, 2, 3000);
		await expect.poll(async () => {
			const p = await positions(page);
			return Math.abs(p[a.start_id].x - p[a.end_id].x);
		}, { message: 'line A vertical after the second solve', timeout: 5000 }).toBeLessThan(tol);

		const p = await positions(page);
		const m = mirror(p[b.start_id], p[a.start_id], p[a.end_id]);
		expect(Math.hypot(m.x - p[b.end_id].x, m.y - p[b.end_id].y), 'still symmetric').toBeLessThan(tol);
		expect(await page.evaluate(() => window.__waffle.getFailedConstraintIndices())).toEqual([]);
	});

});
