/**
 * Constraint modal (constraint-first application). See /specs/constraint_modal.md.
 *
 * Two oracles:
 *  - Branch coverage: drive the pure step engine over LIVE drawn geometry via
 *    __waffle.constraintModalStep and assert action / emitted constraint / run
 *    for unary, chain, rolePair, collect, and reject branches.
 *  - End-to-end: open the Coincident modal and click points through the real
 *    viewport pick loop; assert constraints are created, points converge, an
 *    incompatible pick is ignored with a hint, and Escape closes the modal.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const constraints = (page) => page.evaluate(() => window.__waffle.getConstraints());
const positions = (page) => page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const modal = (page) => page.evaluate(() => window.__waffle.getConstraintModal());
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

/** Draw a point with the point tool at a pixel offset from canvas center. */
async function drawPoint(page, dx, dy) {
	await setTool(page, 'point');
	await clickAt(page, dx, dy);
}

test.describe('constraint modal — branch coverage (pure step over live geometry)', () => {
	test('unary / chain / rolePair / reject all behave per the branch table', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');

		// Two standalone points.
		await drawPoint(page, -60, -40);
		await drawPoint(page, 60, -40);
		// A line (adds two endpoint points + the line).
		await setTool(page, 'line');
		await clickAt(page, -40, 60);
		await clickAt(page, 40, 60);
		// A circle (adds a center point + the circle).
		await setTool(page, 'circle');
		await clickAt(page, 0, -90);
		await clickAt(page, 30, -90);

		const ents = await entities(page);
		const points = ents.filter((e) => e.type === 'Point');
		const lines = ents.filter((e) => e.type === 'Line');
		const circles = ents.filter((e) => e.type === 'Circle');
		expect(points.length).toBeGreaterThanOrEqual(2);
		expect(lines.length).toBe(1);
		expect(circles.length).toBe(1);

		const p0 = points[0].id;
		const p1 = points[1].id;
		const lineId = lines[0].id;
		const circleId = circles[0].id;

		const step = (constraintId, running, pickId) =>
			page.evaluate(
				({ constraintId, running, pickId }) => window.__waffle.constraintModalStep(constraintId, running, pickId),
				{ constraintId, running, pickId }
			);

		// unary: horizontal on a line applies immediately; on a point it rejects.
		const hApply = await step('horizontal', [], lineId);
		expect(hApply.action).toBe('apply');
		expect(hApply.constraints[0].type).toBe('Horizontal');
		expect(hApply.nextRunning).toEqual([]);

		const hReject = await step('horizontal', [], p0);
		expect(hReject.action).toBe('reject');
		expect(hReject.constraints).toHaveLength(0);

		// chain: first point collects, second applies + advances the anchor.
		const cCollect = await step('coincident', [], p0);
		expect(cCollect.action).toBe('collect');
		expect(cCollect.nextRunning).toEqual([p0]);

		const cApply = await step('coincident', [p0], p1);
		expect(cApply.action).toBe('apply');
		expect(cApply.constraints[0].type).toBe('Coincident');
		expect(cApply.nextRunning).toEqual([p1]);

		// chain self-pick and chain-across-incompatible-kind both reject, inertly.
		const cSelf = await step('coincident', [p0], p0);
		expect(cSelf.action).toBe('reject');
		expect(cSelf.nextRunning).toEqual([p0]);

		const cLine = await step('coincident', [p0], lineId);
		expect(cLine.action).toBe('reject');
		expect(cLine.nextRunning).toEqual([p0]);

		// rolePair: midpoint collects a point then applies on the line.
		const mCollect = await step('midpoint', [], p0);
		expect(mCollect.action).toBe('collect');
		const mApply = await step('midpoint', [p0], lineId);
		expect(mApply.action).toBe('apply');
		expect(mApply.constraints[0].type).toBe('Midpoint');

		// rolePair: point-on resolves to OnEntity for a circle.
		const onApply = await step('pointOnLine', [p0], circleId);
		expect(onApply.action).toBe('apply');
		expect(onApply.constraints[0].type).toBe('OnEntity');
	});
});

test.describe('constraint modal — end-to-end pick loop', () => {
	test('Coincident modal chains three points to convergence, ignores a bad pick, Escape closes', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');

		await drawPoint(page, -60, -40);
		await drawPoint(page, 0, -60);
		await drawPoint(page, 60, -40);
		// A far-away line, used as the incompatible pick later.
		await setTool(page, 'line');
		await clickAt(page, -40, 70);
		await clickAt(page, 40, 70);

		const ents = await entities(page);
		const pts = ents.filter((e) => e.type === 'Point').slice(0, 3);
		expect(pts.length).toBe(3);

		const before = (await constraints(page)).filter((c) => c.type === 'Coincident').length;

		// Open the modal (constraint-first) and chain the three points by clicking.
		await page.evaluate(() => window.__waffle.openConstraintModal('coincident'));
		expect((await modal(page))?.constraintId).toBe('coincident');

		await clickAt(page, -60, -40); // p0 → anchor
		await clickAt(page, 0, -60); // p1 → Coincident(p0,p1)
		await clickAt(page, 60, -40); // p2 → Coincident(p1,p2)

		// Two new Coincident constraints chained the three points.
		await expect
			.poll(async () => (await constraints(page)).filter((c) => c.type === 'Coincident').length, { timeout: 5000 })
			.toBe(before + 2);

		// All three points converge (solver welds the chain).
		await expect
			.poll(async () => {
				const pos = await positions(page);
				const a = pos[pts[0].id];
				const b = pos[pts[1].id];
				const c = pos[pts[2].id];
				if (!a || !b || !c) return Infinity;
				return Math.max(Math.hypot(a.x - b.x, a.y - b.y), Math.hypot(b.x - c.x, b.y - c.y));
			}, { timeout: 5000 })
			.toBeLessThan(1e-2);

		// An incompatible pick (the far line) is ignored: count unchanged + a hint.
		const coincidentCount = (await constraints(page)).filter((c) => c.type === 'Coincident').length;
		await clickAt(page, 0, 70); // line midpoint, away from the welded points
		expect((await constraints(page)).filter((c) => c.type === 'Coincident').length).toBe(coincidentCount);
		const m = await modal(page);
		expect(m).not.toBeNull();
		expect((m.message ?? '').length).toBeGreaterThan(0);

		// Escape closes the modal and clears selection.
		await page.keyboard.press('Escape');
		await expect.poll(async () => await modal(page)).toBeNull();
	});
});
