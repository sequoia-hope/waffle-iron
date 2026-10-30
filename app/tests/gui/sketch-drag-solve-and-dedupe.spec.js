/**
 * Sketch drag responsiveness and constraint de-duplication (user report
 * 2026-10-30):
 *
 * 1. "when dragging items to overconstraint, do not buffer/queue the rest of
 *    the drag ... slow motion overconstrained move like every mouse point was
 *    being computed". Every pointer sample used to post a SolveSketch with no
 *    limit, so a slow (over-constrained) solve built a backlog the replies
 *    then replayed. Now at most ONE SolveSketch is in flight: the oracle reads
 *    the engine send log's send and reply times and checks that no two solves
 *    ever overlap.
 * 2. "make sure the point on line constraint does not get applied multiple
 *    times for the same point and line": dropping a point on the same line
 *    twice leaves exactly one OnEntity.
 *
 * Real pointer events only.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickTool } from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle, getCanvasBounds } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';
import { getConstraints } from './helpers/constraint.js';

/** Press at one canvas offset, move in `steps` with no waits, release at another. */
async function fastDrag(page, from, to, steps = 40) {
	const b = await getCanvasBounds(page);
	if (!b) throw new Error('Canvas not visible');
	await page.mouse.move(b.centerX + from[0], b.centerY + from[1]);
	await page.mouse.down();
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(
			b.centerX + from[0] + (to[0] - from[0]) * t,
			b.centerY + from[1] + (to[1] - from[1]) * t
		);
	}
	await page.mouse.up();
}

/** Largest number of SolveSketch requests outstanding at one instant. */
function maxConcurrentSolves(log) {
	const events = [];
	for (const e of log) {
		if (e.type !== 'SolveSketch') continue;
		events.push([e.t, +1]);
		events.push([e.response?.t ?? Number.POSITIVE_INFINITY, -1]);
	}
	// At equal times a reply frees its slot before the next send takes one.
	events.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
	let cur = 0;
	let max = 0;
	for (const [, d] of events) {
		cur += d;
		max = Math.max(max, cur);
	}
	return max;
}

test.describe('sketch drag', () => {
	test('dragging a fixed corner (over-constrained) never has two solves in flight', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickTool(page, 'rectangle');
		await drawRectangle(page, -80, -50, 80, 50);
		await waitForEntityCount(page, 8, 5000);
		await page.keyboard.press('Escape');

		// Fix the corner through the real toolbar, then drag it against the
		// pin: the user's over-constrained case, where each solve is slow.
		await clickAt(page, 80, 50);
		await page.getByTestId('toolbar-btn-constraints-dropdown').click();
		await page.getByTestId('toolbar-constraint-fix').click();
		await expect
			.poll(async () => (await getConstraints(page)).filter((c) => c.type === 'WhereDragged' || c.type === 'Pinned' || c.type === 'Fix').length)
			.toBeGreaterThan(0);

		await page.evaluate(() => window.__waffle.recordEngineSends(true));
		await fastDrag(page, [80, 50], [140, 110], 60);
		// Let the last queued solve go out and answer.
		await expect
			.poll(async () => {
				const log = await page.evaluate(() => window.__waffle.getEngineSendLog());
				return log.filter((e) => e.type === 'SolveSketch' && !e.response).length;
			})
			.toBe(0);
		const log = await page.evaluate(() => window.__waffle.getEngineSendLog());
		await page.evaluate(() => window.__waffle.recordEngineSends(false));

		const solves = log.filter((e) => e.type === 'SolveSketch');
		expect(solves.length, 'the drag solved at all').toBeGreaterThan(0);
		expect(maxConcurrentSolves(log), 'SolveSketch requests in flight at once').toBe(1);
	});

	test('a burst of drag samples sends one solve now and ONE for the newest sample after it', async ({ waffle }) => {
		// On a small sketch the solver answers in ~2 ms, faster than real
		// pointer moves arrive, so the pointer test above cannot build a
		// backlog on this machine. This is the deterministic form of the
		// user's slow over-constrained case: thirty drag samples land before
		// the first solve can answer, exactly as a slow solve sees them. The
		// pointer drag drives the same store path (dragSketchPoint).
		const page = waffle.page;
		await clickSketch(page);
		await clickTool(page, 'rectangle');
		await drawRectangle(page, -80, -50, 80, 50);
		await waitForEntityCount(page, 8, 5000);
		await page.keyboard.press('Escape');

		const target = await page.evaluate(() => {
			const w = window.__waffle;
			const pts = w.getEntities().filter((e) => e.type === 'Point');
			const id = pts[0].id;
			const p0 = w.getPositions().get(id);
			w.recordEngineSends(true);
			let last = null;
			for (let i = 1; i <= 30; i++) {
				last = { x: p0.x + i * 0.0005, y: p0.y + i * 0.0003 };
				w.dragSketchPoint(id, last.x, last.y);
			}
			return { id, ...last };
		});
		await expect
			.poll(async () => {
				const log = await page.evaluate(() => window.__waffle.getEngineSendLog());
				const solves = log.filter((e) => e.type === 'SolveSketch');
				return solves.length > 0 && solves.every((e) => e.response) ? solves.length : -1;
			})
			.toBeGreaterThan(0);
		const log = await page.evaluate(() => window.__waffle.getEngineSendLog());
		await page.evaluate(() => window.__waffle.recordEngineSends(false));

		expect(log.filter((e) => e.type === 'SolveSketch').length, 'solves sent for 30 samples').toBe(2);
		expect(maxConcurrentSolves(log)).toBe(1);
		const p = await page.evaluate((id) => window.__waffle.getPositions().get(id), target.id);
		expect(Math.abs(p.x - target.x), 'lands on the NEWEST sample').toBeLessThan(1e-9);
		expect(Math.abs(p.y - target.y)).toBeLessThan(1e-9);
	});

	test('dropping a point on the same line twice leaves one point-on-line', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await drawLine(page, -120, 0, 120, 0);
		await page.keyboard.press('Escape');
		await clickTool(page, 'point');
		await clickAt(page, -30, -70);
		await page.keyboard.press('Escape');
		await expect.poll(async () => (await page.evaluate(() => window.__waffle.getState().activeTool))).toBe('select');

		const onEntity = async () => (await getConstraints(page)).filter((c) => c.type === 'OnEntity');
		// Drop it on the line, then drag it along the line and drop it again.
		await fastDrag(page, [-30, -70], [-50, 0], 12);
		await expect.poll(async () => (await onEntity()).length, { message: 'first drop binds it' }).toBe(1);
		await fastDrag(page, [-50, 0], [40, 0], 12);
		await page.waitForTimeout(400);
		const after = await onEntity();
		expect(after.length, JSON.stringify(after)).toBe(1);
	});
});
