/**
 * Constraint application through the REAL toolbar, verified by GEOMETRIC EFFECT.
 *
 * Two gaps this file closes.
 *
 * 1. **The toolbar path was barely exercised.** Of the fifteen constraint
 *    buttons only `horizontal`, `parallel` and `angle` were ever clicked by a
 *    test; the other twelve were reached only through
 *    `__waffle.addSketchConstraint(...)`, which bypasses
 *    `getApplicableConstraints()` → `applyConstraint()` entirely. That is the
 *    code deciding WHICH constraint a given selection means — a selection
 *    mapped to the wrong constraint, or a button left permanently disabled,
 *    would not have failed anything.
 *
 * 2. **Effect was not verified.** The existing tests assert that a constraint
 *    OBJECT appears in `getConstraints()`, or that a button is enabled. A
 *    constraint that is recorded but never reaches the solver — or reaches it
 *    and is ignored — passes those assertions. Here every test asserts the
 *    solved GEOMETRY satisfies the constraint: perpendicular lines have a zero
 *    dot product, equal lines have equal lengths, a midpoint lands at the
 *    midpoint.
 *
 * Both drawing paths are covered per the GUI rules: click-click (`drawLine` /
 * `drawCircle`) and click-drag (`dragLine` / `dragCircle`) each feed a
 * constraint, since the two paths build entities through different tool code.
 *
 * Assertions throw on failure — no try/catch around expected state, and no
 * conditional guards that let a missing value pass silently.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickCircle, clickSelect } from './helpers/toolbar.js';
import { drawLine, dragLine, drawCircle, dragCircle } from './helpers/canvas.js';
import { waitForEntityCount, getEntities, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import {
	setSketchSelection,
	clickConstraintButton,
	isConstraintEnabled,
	getConstraints,
} from './helpers/constraint.js';

// ── Helpers ─────────────────────────────────────────────────────────────────

/** Solved positions as a plain object keyed by point id. */
async function positions(page) {
	return page.evaluate(() => {
		const out = {};
		for (const [id, p] of window.__waffle.getPositions()) {
			out[id] = { x: p.x, y: p.y };
		}
		return out;
	});
}

/** Current solve status object (`{ status, dof, failed }`). */
async function solveStatus(page) {
	return page.evaluate(() => window.__waffle.getSolveStatus());
}

/**
 * Wait until the solver publishes a non-failing status.
 *
 * Throws on timeout, reporting the last status seen — a constraint the engine
 * cannot parse leaves this at `null` forever, which is exactly the failure this
 * suite exists to catch, so it must never be swallowed or defaulted.
 */
async function waitForSolved(page, timeout = 8000) {
	const deadline = Date.now() + timeout;
	let last = null;
	while (Date.now() < deadline) {
		last = await solveStatus(page);
		if (last != null && last.status !== 'SolveFailed') {
			await page.waitForTimeout(200);
			return last;
		}
		await page.waitForTimeout(100);
	}
	throw new Error(
		`solver never published a usable status within ${timeout}ms; last = ${JSON.stringify(last)}`
	);
}

function vec(pos, fromId, toId) {
	const a = pos[fromId];
	const b = pos[toId];
	if (!a || !b) throw new Error(`missing solved position for ${fromId} or ${toId}`);
	return { x: b.x - a.x, y: b.y - a.y };
}

function length(v) {
	return Math.hypot(v.x, v.y);
}

/** Draw two independent lines with the click-click path. */
async function twoLinesClickClick(page, a, b) {
	await clickLine(page);
	await drawLine(page, a[0], a[1], a[2], a[3]);
	await waitForEntityCount(page, 3, 5000);
	await page.keyboard.press('Escape');
	await clickLine(page);
	await drawLine(page, b[0], b[1], b[2], b[3]);
	await waitForEntityCount(page, 6, 5000);
}

/** Draw two independent lines with the click-drag path. */
async function twoLinesClickDrag(page, a, b) {
	await clickLine(page);
	await dragLine(page, a[0], a[1], a[2], a[3]);
	await waitForEntityCount(page, 3, 5000);
	await page.keyboard.press('Escape');
	await clickLine(page);
	await dragLine(page, b[0], b[1], b[2], b[3]);
	await waitForEntityCount(page, 6, 5000);
}

/** The two Line entities, asserted to exist. */
async function twoLines(page) {
	const entities = await getEntities(page);
	const lines = entities.filter((e) => e.type === 'Line');
	expect(lines.length, 'expected exactly two lines to have been drawn').toBe(2);
	return lines;
}

test.describe('constraint application via the toolbar, verified by effect', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	// ── Perpendicular (2 lines) ──────────────────────────────────────────────

	test('perpendicular constraint squares two lines — click-click', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Two lines meeting at a shallow angle — clearly NOT perpendicular, and
		// off-axis so snap inference does not pre-apply H/V.
		await twoLinesClickClick(page, [-120, 40, -20, 10], [20, -20, 110, -70]);
		const lines = await twoLines(page);

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id, lines[1].id]);

		expect(
			await isConstraintEnabled(page, 'perpendicular'),
			'perpendicular must be enabled for a two-line selection'
		).toBe(true);

		const before = await positions(page);
		const dotBefore = (() => {
			const u = vec(before, lines[0].start_id, lines[0].end_id);
			const v = vec(before, lines[1].start_id, lines[1].end_id);
			return (u.x * v.x + u.y * v.y) / (length(u) * length(v));
		})();
		expect(
			Math.abs(dotBefore),
			'fixture must start NOT perpendicular, or the test proves nothing'
		).toBeGreaterThan(0.1);

		await clickConstraintButton(page, 'perpendicular');
		await waitForSolved(page);

		const after = await positions(page);
		const u = vec(after, lines[0].start_id, lines[0].end_id);
		const v = vec(after, lines[1].start_id, lines[1].end_id);
		const cos = (u.x * v.x + u.y * v.y) / (length(u) * length(v));
		expect(
			Math.abs(cos),
			`lines must be perpendicular after the constraint (cos=${cos})`
		).toBeLessThan(1e-3);

		expectNoAnyCrash(crashes);
	});

	test('perpendicular constraint squares two lines — click-drag', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await twoLinesClickDrag(page, [-120, 40, -20, 10], [20, -20, 110, -70]);
		const lines = await twoLines(page);

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id, lines[1].id]);
		await clickConstraintButton(page, 'perpendicular');
		await waitForSolved(page);

		const after = await positions(page);
		const u = vec(after, lines[0].start_id, lines[0].end_id);
		const v = vec(after, lines[1].start_id, lines[1].end_id);
		const cos = (u.x * v.x + u.y * v.y) / (length(u) * length(v));
		expect(
			Math.abs(cos),
			`drag-drawn lines must be perpendicular after the constraint (cos=${cos})`
		).toBeLessThan(1e-3);

		expectNoAnyCrash(crashes);
	});

	// ── Equal (2 lines) ──────────────────────────────────────────────────────

	test('equal constraint matches two line lengths — click-click', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Deliberately different lengths: ~100px and ~40px.
		await twoLinesClickClick(page, [-130, 60, -30, 60], [30, -60, 70, -60]);
		const lines = await twoLines(page);

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id, lines[1].id]);

		const before = await positions(page);
		const lenA0 = length(vec(before, lines[0].start_id, lines[0].end_id));
		const lenB0 = length(vec(before, lines[1].start_id, lines[1].end_id));
		expect(
			Math.abs(lenA0 - lenB0) / Math.max(lenA0, lenB0),
			'fixture must start with clearly unequal lengths'
		).toBeGreaterThan(0.2);

		await clickConstraintButton(page, 'equal');
		await waitForSolved(page);

		const after = await positions(page);
		const lenA = length(vec(after, lines[0].start_id, lines[0].end_id));
		const lenB = length(vec(after, lines[1].start_id, lines[1].end_id));
		expect(
			Math.abs(lenA - lenB),
			`line lengths must converge (${lenA} vs ${lenB})`
		).toBeLessThan(1e-3 * Math.max(lenA, lenB));

		expectNoAnyCrash(crashes);
	});

	test('equal constraint matches two line lengths — click-drag', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await twoLinesClickDrag(page, [-130, 60, -30, 60], [30, -60, 70, -60]);
		const lines = await twoLines(page);

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id, lines[1].id]);
		await clickConstraintButton(page, 'equal');
		await waitForSolved(page);

		const after = await positions(page);
		const lenA = length(vec(after, lines[0].start_id, lines[0].end_id));
		const lenB = length(vec(after, lines[1].start_id, lines[1].end_id));
		expect(
			Math.abs(lenA - lenB),
			`drag-drawn line lengths must converge (${lenA} vs ${lenB})`
		).toBeLessThan(1e-3 * Math.max(lenA, lenB));

		expectNoAnyCrash(crashes);
	});

	// ── Midpoint (1 point + 1 line) ──────────────────────────────────────────

	test('midpoint constraint centres a point on a line', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Line 1 is the target; line 2 contributes a free endpoint to constrain.
		await twoLinesClickClick(page, [-120, 60, 120, 60], [-60, -70, 40, -40]);
		const lines = await twoLines(page);

		const entities = await getEntities(page);
		const line1Points = new Set([lines[0].start_id, lines[0].end_id]);
		const freePoint = entities.find((e) => e.type === 'Point' && !line1Points.has(e.id));
		expect(freePoint, 'expected a point not belonging to the target line').toBeTruthy();

		await clickSelect(page);
		await setSketchSelection(page, [freePoint.id, lines[0].id]);

		expect(
			await isConstraintEnabled(page, 'midpoint'),
			'midpoint must be enabled for a point+line selection'
		).toBe(true);

		await clickConstraintButton(page, 'midpoint');
		await waitForSolved(page);

		const after = await positions(page);
		const a = after[lines[0].start_id];
		const b = after[lines[0].end_id];
		const p = after[freePoint.id];
		const midX = (a.x + b.x) / 2;
		const midY = (a.y + b.y) / 2;
		const lineLen = length({ x: b.x - a.x, y: b.y - a.y });
		expect(lineLen, 'target line must not have collapsed').toBeGreaterThan(1e-5);
		expect(
			Math.hypot(p.x - midX, p.y - midY) / lineLen,
			`point must land at the line midpoint (${p.x},${p.y}) vs (${midX},${midY})`
		).toBeLessThan(1e-4);

		expectNoAnyCrash(crashes);
	});

	// ── Point on line (1 point + 1 line) ─────────────────────────────────────

	test('point-on-line constraint drops a point onto the line', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await twoLinesClickClick(page, [-120, 60, 120, 60], [-60, -70, 40, -40]);
		const lines = await twoLines(page);

		const entities = await getEntities(page);
		const line1Points = new Set([lines[0].start_id, lines[0].end_id]);
		const freePoint = entities.find((e) => e.type === 'Point' && !line1Points.has(e.id));
		expect(freePoint, 'expected a point not belonging to the target line').toBeTruthy();

		await clickSelect(page);
		await setSketchSelection(page, [freePoint.id, lines[0].id]);
		await clickConstraintButton(page, 'pointOnLine');
		await waitForSolved(page);

		const after = await positions(page);
		const a = after[lines[0].start_id];
		const b = after[lines[0].end_id];
		const p = after[freePoint.id];
		const lx = b.x - a.x;
		const ly = b.y - a.y;
		const lLen = Math.hypot(lx, ly);
		expect(lLen, 'target line must not have collapsed').toBeGreaterThan(1e-5);
		const perp = Math.abs((p.x - a.x) * ly - (p.y - a.y) * lx) / lLen;
		expect(
			perp / lLen,
			`point must lie on the line (perpendicular distance ${perp}, line length ${lLen})`
		).toBeLessThan(1e-4);

		expectNoAnyCrash(crashes);
	});

	// ── Symmetric H / V (2 points) ───────────────────────────────────────────

	test('symmetric-horizontal constraint mirrors two points about the Y axis', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Asymmetric on purpose: different |x| and different y.
		await clickLine(page);
		await drawLine(page, -140, 30, 60, -50);
		await waitForEntityCount(page, 3, 5000);

		const lines = (await getEntities(page)).filter((e) => e.type === 'Line');
		expect(lines.length).toBe(1);
		const [pa, pb] = [lines[0].start_id, lines[0].end_id];

		await clickSelect(page);
		await setSketchSelection(page, [pa, pb]);

		expect(
			await isConstraintEnabled(page, 'symmetricH'),
			'symmetricH must be enabled for a two-point selection'
		).toBe(true);

		await clickConstraintButton(page, 'symmetricH');
		await waitForSolved(page);

		const after = await positions(page);
		const scale = Math.max(Math.abs(after[pa].x), Math.abs(after[pa].y));
		expect(scale, 'points must not have collapsed to the origin').toBeGreaterThan(1e-5);
		expect(
			Math.abs(after[pa].x + after[pb].x) / scale,
			`x coordinates must be opposite (${after[pa].x} + ${after[pb].x})`
		).toBeLessThan(1e-4);
		expect(
			Math.abs(after[pa].y - after[pb].y) / scale,
			`y coordinates must match (${after[pa].y} vs ${after[pb].y})`
		).toBeLessThan(1e-4);

		expectNoAnyCrash(crashes);
	});

	test('symmetric-vertical constraint mirrors two points about the X axis', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await clickLine(page);
		await drawLine(page, -140, 30, 60, -50);
		await waitForEntityCount(page, 3, 5000);

		const lines = (await getEntities(page)).filter((e) => e.type === 'Line');
		const [pa, pb] = [lines[0].start_id, lines[0].end_id];

		await clickSelect(page);
		await setSketchSelection(page, [pa, pb]);
		await clickConstraintButton(page, 'symmetricV');
		await waitForSolved(page);

		const after = await positions(page);
		const scale = Math.max(Math.abs(after[pa].x), Math.abs(after[pa].y));
		expect(scale, 'points must not have collapsed to the origin').toBeGreaterThan(1e-5);
		expect(
			Math.abs(after[pa].x - after[pb].x) / scale,
			`x coordinates must match (${after[pa].x} vs ${after[pb].x})`
		).toBeLessThan(1e-4);
		expect(
			Math.abs(after[pa].y + after[pb].y) / scale,
			`y coordinates must be opposite (${after[pa].y} + ${after[pb].y})`
		).toBeLessThan(1e-4);

		expectNoAnyCrash(crashes);
	});

	// ── Coincident (2 points) ────────────────────────────────────────────────

	test('coincident constraint merges two separate endpoints', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await twoLinesClickClick(page, [-130, -40, -50, -40], [50, 50, 130, 50]);
		const lines = await twoLines(page);

		const ptA = lines[0].end_id;
		const ptB = lines[1].start_id;

		await clickSelect(page);
		await setSketchSelection(page, [ptA, ptB]);

		const before = await positions(page);
		const gap0 = Math.hypot(before[ptA].x - before[ptB].x, before[ptA].y - before[ptB].y);
		expect(gap0, 'fixture must start with a real gap between the endpoints').toBeGreaterThan(1e-4);

		await clickConstraintButton(page, 'coincident');
		await waitForSolved(page);

		const after = await positions(page);
		const gap = Math.hypot(after[ptA].x - after[ptB].x, after[ptA].y - after[ptB].y);
		expect(
			gap / gap0,
			`endpoints must coincide (gap went ${gap0} → ${gap})`
		).toBeLessThan(1e-4);

		expectNoAnyCrash(crashes);
	});

	// ── Vertical (1 line) ────────────────────────────────────────────────────

	test('vertical constraint stands a slanted line upright', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Slanted enough that snap inference does not pre-apply Vertical.
		await clickLine(page);
		await drawLine(page, -60, 80, 40, -80);
		await waitForEntityCount(page, 3, 5000);

		const lines = (await getEntities(page)).filter((e) => e.type === 'Line');
		expect(lines.length).toBe(1);

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id]);
		await clickConstraintButton(page, 'vertical');
		await waitForSolved(page);

		const after = await positions(page);
		const d = vec(after, lines[0].start_id, lines[0].end_id);
		expect(Math.abs(d.y), 'line must not have collapsed to a point').toBeGreaterThan(1e-5);
		expect(
			Math.abs(d.x) / Math.abs(d.y),
			`line must be vertical: Δx=${d.x}, Δy=${d.y}`
		).toBeLessThan(1e-4);

		expectNoAnyCrash(crashes);
	});

	// ── Tangent (1 line + 1 circle) ──────────────────────────────────────────

	test('tangent constraint brings a line to touch a circle — click-drag circle', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Circle drawn with the click-drag path; line with click-click.
		await clickCircle(page);
		await dragCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		await clickLine(page);
		await drawLine(page, -140, 110, 140, 130);
		await waitForEntityCount(page, 5, 5000);

		const entities = await getEntities(page);
		const circles = entities.filter((e) => e.type === 'Circle');
		const lines = entities.filter((e) => e.type === 'Line');
		expect(circles.length, 'expected one circle').toBe(1);
		expect(lines.length, 'expected one line').toBe(1);

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id, circles[0].id]);

		expect(
			await isConstraintEnabled(page, 'tangent'),
			'tangent must be enabled for a line+circle selection'
		).toBe(true);

		await clickConstraintButton(page, 'tangent');
		await waitForSolved(page);

		const after = await positions(page);
		const centre = after[circles[0].center_id];
		const a = after[lines[0].start_id];
		const b = after[lines[0].end_id];
		const lx = b.x - a.x;
		const ly = b.y - a.y;
		const lLen = Math.hypot(lx, ly);
		expect(lLen, 'line must not have collapsed').toBeGreaterThan(1e-6);
		const dist = Math.abs((centre.x - a.x) * ly - (centre.y - a.y) * lx) / lLen;

		// The solved radius reaches the entity list, not `positions`.
		const solvedCircle = (await getEntities(page)).find((e) => e.id === circles[0].id);
		const radius = solvedCircle.radius;
		expect(radius, 'circle must retain a positive radius').toBeGreaterThan(1e-6);
		expect(
			Math.abs(dist - radius),
			`distance from centre to line (${dist}) must equal the radius (${radius})`
		).toBeLessThan(1e-2 * radius);

		expectNoAnyCrash(crashes);
	});

	// ── Equal on two circles (click-click and click-drag) ────────────────────

	test('equal constraint matches two circle radii — mixed click-click and click-drag', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// One circle per drawing path, and deliberately different sizes.
		await clickCircle(page);
		await drawCircle(page, -90, 0, -30, 0); // click-click, r ≈ 60px
		await waitForEntityCount(page, 2, 5000);

		await clickCircle(page);
		await dragCircle(page, 90, 0, 115, 0); // click-drag, r ≈ 25px
		await waitForEntityCount(page, 4, 5000);

		const entities = await getEntities(page);
		const circles = entities.filter((e) => e.type === 'Circle');
		expect(circles.length, 'expected two circles').toBe(2);
		expect(
			Math.abs(circles[0].radius - circles[1].radius) / Math.max(circles[0].radius, circles[1].radius),
			'fixture must start with clearly unequal radii'
		).toBeGreaterThan(0.2);

		await clickSelect(page);
		await setSketchSelection(page, [circles[0].id, circles[1].id]);
		await clickConstraintButton(page, 'equal');
		await waitForSolved(page);

		const solved = (await getEntities(page)).filter((e) => e.type === 'Circle');
		expect(solved.length).toBe(2);
		expect(
			Math.abs(solved[0].radius - solved[1].radius),
			`radii must converge (${solved[0].radius} vs ${solved[1].radius})`
		).toBeLessThan(1e-2 * Math.max(solved[0].radius, solved[1].radius));

		expectNoAnyCrash(crashes);
	});

	// ── Fix (1 point) ────────────────────────────────────────────────────────

	test('fix constraint removes the two degrees of freedom of a point', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await clickLine(page);
		await drawLine(page, -60, 80, 40, -80);
		await waitForEntityCount(page, 3, 5000);

		const lines = (await getEntities(page)).filter((e) => e.type === 'Line');
		const target = lines[0].start_id;

		// The solver publishes nothing until the sketch has at least one
		// constraint, so establish a baseline solve first. Vertical also
		// leaves the line non-degenerate for the position check below.
		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id]);
		await clickConstraintButton(page, 'vertical');
		const before = await waitForSolved(page);
		expect(
			typeof before.dof,
			'solve status must report a numeric dof — the UI badge depends on it'
		).toBe('number');

		await clickSelect(page);
		await setSketchSelection(page, [target]);
		expect(
			await isConstraintEnabled(page, 'fix'),
			'fix must be enabled for a single-point selection'
		).toBe(true);

		const posBefore = await positions(page);
		await clickConstraintButton(page, 'fix');
		await waitForSolved(page);

		const after = await solveStatus(page);
		expect(
			after.dof,
			`fixing a point must remove 2 dof (was ${before.dof}, now ${after.dof})`
		).toBe(before.dof - 2);

		// A fix pins the point where it already was — it must not move it.
		const posAfter = await positions(page);
		expect(
			Math.hypot(posAfter[target].x - posBefore[target].x, posAfter[target].y - posBefore[target].y),
			'fix must pin the point in place, not relocate it'
		).toBeLessThan(1e-6);

		expectNoAnyCrash(crashes);
	});

	// ── The constraint actually lands in the sketch ──────────────────────────

	test('a toolbar-applied constraint is recorded exactly once', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await twoLinesClickClick(page, [-120, 40, -20, 10], [20, -20, 110, -70]);
		const lines = await twoLines(page);

		const before = (await getConstraints(page)).length;

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id, lines[1].id]);
		await clickConstraintButton(page, 'perpendicular');
		await waitForSolved(page);

		const after = await getConstraints(page);
		expect(
			after.length,
			'one toolbar click must add exactly one constraint'
		).toBe(before + 1);
		expect(
			after.filter((c) => c.type === 'Perpendicular').length,
			'the recorded constraint must be a Perpendicular'
		).toBe(1);

		expectNoAnyCrash(crashes);
	});
});
