/**
 * Snap priority rework + point-alignment inference.
 * Contract: specs/snap_inference_and_priority.md — oracles O1–O6, O8, O9;
 * invariants I1–I8. (O7 is the existing-suite regression run, not a test here.)
 *
 * REAL POINTER EVENTS ONLY — sketch space is screen-mapped, so pixel offsets
 * (canvas.js clickAt/moveTo/drawLine) are the coordinates, NOT worldToScreen.
 * Snap state is read via __waffle.getSnapIndicator(); constraints via
 * getConstraints(); solved coordinates via getEntities() point positions.
 *
 * EXPECTED RED on current code (feature not implemented):
 *   O1  — on-entity currently loses to segment-H/V (indicator 'horizontal').
 *   O2  — OnEntity template is dropped at emission (no OnEntity constraint).
 *   O3/O3v/O4/O6/O9 — align inference does not exist (getSnapIndicator never
 *         returns 'align-h'/'align-v').
 *   O5  — control (I4): passes trivially now (no inference), guards I4 later.
 *
 * I5 (dashed inference line) is asserted at the indicator level — no renderable
 * snapGeo/getInferenceSources hook is exposed to tests (noted in the report);
 * the align-h/align-v indicator is the visual contract's store-level witness.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine } from './helpers/toolbar.js';
import { clickAt, moveTo, drawLine } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';
import { getConstraints } from './helpers/constraint.js';

const indicator = (page) => page.evaluate(() => window.__waffle.getSnapIndicator());
const pxSize = (page) => page.evaluate(() => window.__waffle.getSketchPixelSize());
const constraintsByType = async (page, type) =>
	(await getConstraints(page)).filter((c) => c.type === type);
/** All sketch Point entities with their solved positions. */
const pointPos = (page) =>
	page.evaluate(() =>
		window.__waffle.getEntities().filter((e) => e.type === 'Point').map((p) => ({ id: p.id, x: p.x, y: p.y }))
	);

/** Activate the Point tool via its toolbar button (real click). */
async function clickPoint(page) {
	await page.locator('[data-testid="toolbar-btn-point"]').click();
	await page.waitForFunction(() => window.__waffle?.getState()?.activeTool === 'point', { timeout: 3000 });
}

test.describe('snap priority + alignment inference', () => {
	// ---- O1 (I1): on-entity outranks segment-direction H/V.
	test('O1: on-entity wins over segment-H/V within the H/V wedge', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickLine(page);

		// Host VERTICAL line at x=+60 (endpoints at y=±120, midpoint at (60,0)).
		await drawLine(page, 60, -120, 60, 120);
		await waitForEntityCount(page, 3, 3000);

		// New line from S=(-60, 80); hover onto the host line at (60, 80): the
		// segment S→cursor is exactly horizontal (0°, inside the 3° wedge) AND the
		// cursor lies on the host line (0px), clear of its midpoint/endpoints.
		await clickAt(page, -60, 80);
		await page.waitForTimeout(200);
		await moveTo(page, 60, 80);
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'on-entity outranks horizontal in the wedge (I1)').toBe('on-entity');

		// Control: same horizontal direction into free space → still 'horizontal'.
		await moveTo(page, 220, 80);
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'free-space direction still snaps horizontal').toBe('horizontal');
	});

	// ---- O2 (I2): on-entity placement emits OnEntity and is parametric.
	test('O2: on-entity placement emits OnEntity and stays on the host line when dragged', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0); // host horizontal line at y=0
		await waitForEntityCount(page, 3, 3000);

		// Point tool → hover the host line interior at (40,0): on-entity (no
		// fromPoint, so no H/V competition), clear of endpoints/midpoint/origin.
		await clickPoint(page);
		await moveTo(page, 40, 0);
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'cursor on the line interior → on-entity').toBe('on-entity');

		const before = await pointPos(page);
		await clickAt(page, 40, 0);
		await page.waitForTimeout(300);

		const oe = await constraintsByType(page, 'OnEntity');
		expect(oe.length, 'placing via on-entity emits an OnEntity constraint (I2)').toBeGreaterThanOrEqual(1);

		// The new point Q.
		const after = await pointPos(page);
		const q = after.find((p) => !before.some((b) => b.id === p.id));
		expect(q, 'a new point was placed on the line').toBeTruthy();

		// Drag the whole host line up and re-solve; Q must remain on it.
		const ps = await pxSize(page);
		const hostId = await page.evaluate(() => {
			const line = window.__waffle.getEntities().find((e) => e.type === 'Line');
			return line ? line.id : null;
		});
		expect(hostId, 'host line id resolved').not.toBeNull();
		await page.evaluate(
			({ id, dy }) => {
				window.__waffle.dragSketchLine(id, 0, dy);
				window.__waffle.finalizeDrag();
			},
			{ id: hostId, dy: 40 * ps }
		);
		await page.waitForTimeout(400);

		const dist = await page.evaluate((qid) => {
			const es = window.__waffle.getEntities();
			const pos = Object.fromEntries(window.__waffle.getPositions());
			const line = es.find((e) => e.type === 'Line');
			const a = pos[line.start_id], b = pos[line.end_id], q = pos[qid];
			const abx = b.x - a.x, aby = b.y - a.y;
			const L = Math.hypot(abx, aby) || 1;
			return Math.abs((q.x - a.x) * aby - (q.y - a.y) * abx) / L;
		}, q.id);
		expect(dist, 'point stays on the host line after it is dragged (OnEntity parametric)').toBeLessThan(2 * ps);
	});

	// ---- O3 (I3/I5): align-h inference.
	test('O3: align-h emits HorizontalPoints and equates y to the armed source', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickPoint(page);

		// Place source P at (-60, 40).
		await clickAt(page, -60, 40);
		await page.waitForTimeout(200);
		const P = (await pointPos(page))[0];
		expect(P, 'source point P placed').toBeTruthy();

		// Arm P by hovering it (coincident snap).
		await moveTo(page, 220, -200);
		await moveTo(page, -60, 40);
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'hovering P shows coincident (arming precondition)').toBe('coincident');

		// Enter P's horizontal band 4px off its axis (within INFERENCE_ALIGN_PX=6),
		// x far from P.
		await moveTo(page, 220, -200);
		await moveTo(page, 60, 44);
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'cursor in armed P horizontal band → align-h (I3/I5)').toBe('align-h');

		const before = await pointPos(page);
		await clickAt(page, 60, 44);
		await page.waitForTimeout(300);

		const hp = await constraintsByType(page, 'HorizontalPoints');
		expect(hp.length, 'clicking on align-h emits HorizontalPoints').toBeGreaterThanOrEqual(1);

		const ps = await pxSize(page);
		const after = await pointPos(page);
		const q = after.find((p) => !before.some((b) => b.id === p.id));
		expect(q, 'aligned point Q placed').toBeTruthy();
		expect(Math.abs(q.y - P.y), 'solved Q.y equals source P.y (I3)').toBeLessThan(2 * ps);
	});

	// ---- O3 mirror: align-v.
	test('O3v: align-v emits VerticalPoints and equates x to the armed source', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickPoint(page);

		await clickAt(page, -60, 40); // source P
		await page.waitForTimeout(200);
		const P = (await pointPos(page))[0];

		await moveTo(page, 220, -200);
		await moveTo(page, -60, 40);
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'hovering P shows coincident (arming)').toBe('coincident');

		// P's vertical band 4px off its axis, y far from P.
		await moveTo(page, 220, -200);
		await moveTo(page, -56, -60);
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'cursor in armed P vertical band → align-v').toBe('align-v');

		const before = await pointPos(page);
		await clickAt(page, -56, -60);
		await page.waitForTimeout(300);

		const vp = await constraintsByType(page, 'VerticalPoints');
		expect(vp.length, 'clicking on align-v emits VerticalPoints').toBeGreaterThanOrEqual(1);

		const ps = await pxSize(page);
		const after = await pointPos(page);
		const q = after.find((p) => !before.some((b) => b.id === p.id));
		expect(Math.abs(q.x - P.x), 'solved Q.x equals source P.x').toBeLessThan(2 * ps);
	});

	// ---- O4: both bands at once (align-h + align-v combined).
	test('O4: intersection of two armed bands emits both point-pair constraints', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickPoint(page);

		await clickAt(page, -80, 30); // A (defines a horizontal axis y=30)
		await page.waitForTimeout(150);
		await clickAt(page, 50, -70); // B (defines a vertical axis x=50)
		await page.waitForTimeout(150);
		const pts = await pointPos(page);
		const A = pts.find((p) => p.id === pts[0].id); // first placed
		expect(pts.length, 'two source points').toBe(2);

		// Arm A then B.
		await moveTo(page, 220, 200);
		await moveTo(page, -80, 30);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, 'A armed (coincident)').toBe('coincident');
		await moveTo(page, 220, 200);
		await moveTo(page, 50, -70);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, 'B armed (coincident)').toBe('coincident');

		// Move to (B.x, A.y) = (50, 30): on A's horizontal axis AND B's vertical axis.
		await moveTo(page, 220, 200);
		await moveTo(page, 50, 30);
		await page.waitForTimeout(200);
		const ind = await indicator(page);
		expect(['align-h', 'align-v'], 'combined band shows an align indicator').toContain(ind?.type);

		const before = await pointPos(page);
		await clickAt(page, 50, 30);
		await page.waitForTimeout(300);

		expect((await constraintsByType(page, 'HorizontalPoints')).length, 'HorizontalPoints{A,new}').toBeGreaterThanOrEqual(1);
		expect((await constraintsByType(page, 'VerticalPoints')).length, 'VerticalPoints{B,new}').toBeGreaterThanOrEqual(1);

		const ps = await pxSize(page);
		const after = await pointPos(page);
		const q = after.find((p) => !before.some((b) => b.id === p.id));
		const Apos = before.find((p) => p.id === A.id);
		const Bpos = before.find((p) => p.id === pts[1].id);
		expect(Math.abs(q.y - Apos.y), 'solved y equals A.y').toBeLessThan(2 * ps);
		expect(Math.abs(q.x - Bpos.x), 'solved x equals B.x').toBeLessThan(2 * ps);
	});

	// ---- O5 (I4 control): no arming → no align. Passes now; guards I4 later.
	test('O5: alignment never appears for a point that was not hovered (I4)', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickPoint(page);

		// Place P but never hover it (placement must not arm).
		await clickAt(page, -60, 40);
		await moveTo(page, 220, -200); // leave P without a return hover

		// Enter where P's horizontal band would be — P is not armed.
		await moveTo(page, 60, 44);
		await page.waitForTimeout(200);
		const ind = await indicator(page);
		expect(ind?.type === 'align-h', 'no align-h without deliberate arming').toBe(false);
		expect(ind?.type === 'align-v', 'no align-v without deliberate arming').toBe(false);
	});

	// ---- O6 (LRU): 4 armed sources, INFERENCE_SOURCES_MAX=3 → 1st evicted.
	test('O6: oldest of four armed sources is evicted (LRU=3)', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickPoint(page);

		// Four well-separated points (distinct x and y).
		const P = [[-100, 20], [-40, -40], [40, 60], [100, -20]];
		for (const [x, y] of P) {
			await clickAt(page, x, y);
			await page.waitForTimeout(120);
		}

		// Arm in order P1..P4, routing through a far waypoint so no hover crosses
		// another point (keeps the LRU order deterministic).
		for (const [x, y] of P) {
			await moveTo(page, 0, 250);
			await moveTo(page, x, y);
			await page.waitForTimeout(120);
			expect((await indicator(page))?.type, `armed point (${x},${y})`).toBe('coincident');
		}

		// The most-recent source P4 (y=-20) still aligns.
		await moveTo(page, 0, 250);
		await moveTo(page, 0, -18); // 2px off P4's axis, x far from P4
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type, 'most-recent armed source P4 aligns (align-h)').toBe('align-h');

		// The oldest source P1 (y=20) was evicted → no align.
		await moveTo(page, 0, 250);
		await moveTo(page, 0, 18); // 2px off P1's axis
		await page.waitForTimeout(200);
		expect((await indicator(page))?.type === 'align-h', 'evicted source P1 does not align').toBe(false);
	});

	// ---- O9 (I6): the align-h pointer sequence is deterministic across repeats.
	// Each run starts from a FRESH sketch and replays the IDENTICAL sequence, so
	// run 2 cannot be contaminated by run 1's placed point (an earlier version
	// clicked 4px from run 1's Q1, inside COINCIDENT_SNAP_PX, so coincident won
	// run 2 — the runs weren't independent). Same sequence from same state ⇒ same
	// indicator + same constraint delta.
	test('O9: replaying the align-h sequence from a fresh sketch is deterministic', async ({ waffle }) => {
		const page = waffle.page;

		const runAlignH = async () => {
			// Fresh, empty sketch — identical starting state for every run.
			await page.evaluate(() => window.__waffle.exitSketch());
			await clickSketch(page);
			await clickPoint(page);
			await clickAt(page, -60, 40); // source P (same coords each run)
			await page.waitForTimeout(150);

			const before = (await constraintsByType(page, 'HorizontalPoints')).length;
			await moveTo(page, 220, -200);
			await moveTo(page, -60, 40); // arm P
			await page.waitForTimeout(120);
			await moveTo(page, 220, -200);
			await moveTo(page, 60, 44); // align band
			await page.waitForTimeout(150);
			const indType = (await indicator(page))?.type ?? null;
			await clickAt(page, 60, 44); // place aligned point
			await page.waitForTimeout(200);
			const after = (await constraintsByType(page, 'HorizontalPoints')).length;
			return { indType, hpDelta: after - before };
		};

		const run1 = await runAlignH();
		const run2 = await runAlignH();

		expect(run1.indType, 'run1 indicator is align-h').toBe('align-h');
		expect(run2.indType, 'run2 indicator equals run1 (determinism)').toBe(run1.indType);
		expect(run2.hpDelta, 'run2 emits the same number of HorizontalPoints as run1').toBe(run1.hpDelta);
		expect(run1.hpDelta, 'each run emits exactly one HorizontalPoints from its fresh sketch').toBe(1);
	});
});
