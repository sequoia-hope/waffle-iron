/**
 * Dimension tool — pick-then-place with orientation heuristic.
 * See /specs/dimension_tool.md.
 *
 *  - Branch coverage: drive the pure classifier (__waffle.classifyDimension)
 *    over live drawn geometry for every row of the branch table + a mutation
 *    check (side vs above leader flips vertical↔horizontal).
 *  - End-to-end: pick two points, place the leader to the side, confirm the
 *    popup, and assert a VDistance constraint is created and solved.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const constraints = (page) => page.evaluate(() => window.__waffle.getConstraints());
const positions = (page) => page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

async function drawPoint(page, dx, dy) {
	await setTool(page, 'point');
	await clickAt(page, dx, dy);
}
async function drawLineSeg(page, x1, y1, x2, y2) {
	// Re-arm via select so the chained line tool starts a fresh segment (calling
	// setTool('line') while already on 'line' would not reset the pending start).
	await setTool(page, 'select');
	await setTool(page, 'line');
	await clickAt(page, x1, y1);
	await clickAt(page, x2, y2);
}

const classify = (page, targets, leader) =>
	page.evaluate(({ targets, leader }) => window.__waffle.classifyDimension(targets, leader), { targets, leader });

test.describe('dimension tool — classifier branch coverage', () => {
	test('linear H/V/aligned, single-line, point-line, line-line distance & angle', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');

		// Two standalone points offset in BOTH axes.
		await drawPoint(page, -70, -50);
		await drawPoint(page, 50, 40);
		// La, Lb horizontal & parallel; Lc diagonal (crosses La's direction).
		await drawLineSeg(page, -70, 80, 70, 80);
		await drawLineSeg(page, -70, 55, 70, 55);
		await drawLineSeg(page, -70, -80, 70, -30);

		const ents = await entities(page);
		const pts = ents.filter((e) => e.type === 'Point');
		const lines = ents.filter((e) => e.type === 'Line');
		expect(lines.length).toBe(3);
		const P0 = pts[0].id;
		const P1 = pts[1].id;
		const La = lines[0].id;
		const Lb = lines[1].id;
		const Lc = lines[2].id;

		const pos = await positions(page);
		const a = pos[P0];
		const b = pos[P1];
		const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
		const SPAN = Math.hypot(b.x - a.x, b.y - a.y) + 1;

		const pt = (id) => ({ id, type: 'Point' });
		const ln = (id) => ({ id, type: 'Line' });

		// Leader to the SIDE (large Δx offset) → vertical → measures |Δy|.
		const sideLeader = { x: mid.x + SPAN, y: mid.y + SPAN * 0.01 };
		const vert = await classify(page, [pt(P0), pt(P1)], sideLeader);
		expect(vert.dimKind).toBe('linear');
		expect(vert.orientation).toBe('vertical');
		expect(vert.constraint.type).toBe('VDistance');
		expect(vert.value).toBeCloseTo(Math.abs(b.y - a.y), 3);

		// Leader ABOVE (large Δy offset) → horizontal → measures |Δx|.
		const aboveLeader = { x: mid.x + SPAN * 0.01, y: mid.y + SPAN };
		const horiz = await classify(page, [pt(P0), pt(P1)], aboveLeader);
		expect(horiz.orientation).toBe('horizontal');
		expect(horiz.constraint.type).toBe('HDistance');
		expect(horiz.value).toBeCloseTo(Math.abs(b.x - a.x), 3);

		// Mutation: same two points, different leader → different orientation.
		expect(vert.orientation).not.toBe(horiz.orientation);

		// Diagonal leader (~45°) → aligned → straight-line distance.
		const diagLeader = { x: mid.x + SPAN, y: mid.y + SPAN };
		const aligned = await classify(page, [pt(P0), pt(P1)], diagLeader);
		expect(aligned.orientation).toBe('aligned');
		expect(aligned.constraint.type).toBe('Distance');
		expect(aligned.value).toBeCloseTo(Math.hypot(b.x - a.x, b.y - a.y), 3);

		// Single line → linear length dimension.
		const single = await classify(page, [ln(La)], { x: 0, y: 1 });
		expect(single.dimKind).toBe('linear');
		expect(['HDistance', 'VDistance', 'Distance']).toContain(single.constraint.type);

		// Point + line → perpendicular distance.
		const perp = await classify(page, [pt(P0), ln(La)], { x: 0, y: 0 });
		expect(perp.dimKind).toBe('perp');
		expect(perp.constraint.type).toBe('PointLineDistance');

		// Two parallel lines → distance.
		const lineDist = await classify(page, [ln(La), ln(Lb)], { x: 0, y: 0 });
		expect(lineDist.dimKind).toBe('lineDistance');
		expect(lineDist.constraint.type).toBe('PointLineDistance');

		// Line + crossing line → angle.
		const ang = await classify(page, [ln(La), ln(Lc)], { x: 0, y: 0 });
		expect(ang.dimKind).toBe('angle');
		expect(ang.constraint.type).toBe('Angle');
		expect(ang.value).toBeGreaterThan(0);
	});
});

test.describe('dimension tool — end-to-end place + solve', () => {
	test('two points, side leader → VDistance created and solved to the entered value', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');

		await drawPoint(page, -60, -40);
		await drawPoint(page, 40, 30);
		const pts = (await entities(page)).filter((e) => e.type === 'Point').slice(0, 2);
		const [P0, P1] = [pts[0].id, pts[1].id];

		// Pick both points with the dimension tool.
		await setTool(page, 'dimension');
		await clickAt(page, -60, -40);
		await clickAt(page, 40, 30);

		// Place the leader far to the side (free space) → vertical dimension.
		await clickAt(page, 150, -5);

		// The value popup opens, pre-filled with the measured |Δy|.
		const popup = await page.evaluate(() => window.__waffle.getDimensionPopup());
		expect(popup).not.toBeNull();
		expect(popup.dimType).toBe('custom');
		expect(popup.defaultValue).toBeGreaterThan(0);

		// Confirm with a DIFFERENT value (1.5×) and assert the solver enforces it.
		const target = parseFloat((popup.defaultValue * 1.5).toFixed(6));
		await page.evaluate((v) => window.__waffle.applyDimensionFromPopup(v), target);

		await expect
			.poll(async () => (await constraints(page)).some((c) => c.type === 'VDistance'), { timeout: 5000 })
			.toBe(true);

		await expect
			.poll(async () => {
				const pos = await positions(page);
				const a = pos[P0];
				const b = pos[P1];
				if (!a || !b) return Infinity;
				return Math.abs(Math.abs(b.y - a.y) - target);
			}, { timeout: 5000 })
			.toBeLessThan(Math.max(6e-3, target * 0.05));
	});
});
