/**
 * ADVERSARIAL probes for snap priority rework + point-alignment inference.
 * Contract: specs/snap_inference_and_priority.md (cascade, branch table, I1–I8,
 * failure modes). Sibling of snap-inference.spec.js (feature oracles O1–O9).
 *
 * REAL POINTER EVENTS ONLY. Sketch space is screen-mapped, so clickAt/moveTo
 * take screen-pixel offsets from canvas center (NOT worldToScreen). Snap state
 * is read via __waffle.getSnapIndicator(); armed sources via
 * getInferenceSources(); constraints via getConstraints(); solved coords via
 * getEntities() point positions; over-constraint via getOverConstrained().
 *
 * These push the flow into pathological corners the feature oracles skip:
 * armed-source lifecycle (delete/undo), LRU clearing + stress, zoom during
 * inference, over-constraint avoidance, on-entity on a circle, non-line
 * placement paths, two-source both-bands, fromPointId self-suppression, and the
 * on-entity≻align priority sandwich. Where a probe documents a deviation from
 * the spec's stated intent, the assertion matches the SPEC and the comment
 * flags it as a finding (working tree is never modified).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine } from './helpers/toolbar.js';
import { clickAt, moveTo, drawLine } from './helpers/canvas.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { getConstraints } from './helpers/constraint.js';

const indicator = (p) => p.evaluate(() => window.__waffle.getSnapIndicator());
const inferSources = (p) => p.evaluate(() => window.__waffle.getInferenceSources());
const overConstrained = (p) => p.evaluate(() => window.__waffle.getOverConstrained());
const pxSize = (p) => p.evaluate(() => window.__waffle.getSketchPixelSize());
const cbt = async (p, t) => (await getConstraints(p)).filter((c) => c.type === t);
const pointPos = (p) =>
	p.evaluate(() => window.__waffle.getEntities().filter((e) => e.type === 'Point').map((x) => ({ id: x.id, x: x.x, y: x.y })));
const toolOf = (p) => p.evaluate(() => window.__waffle.getState().activeTool);
const screenOff = (p, sx, sy) => p.evaluate(({ sx, sy }) => window.__waffle.sketchToScreenOffset(sx, sy), { sx, sy });

/** Activate a tool by toolbar button and wait for it to be active. */
async function activateTool(page, id) {
	await page.locator(`[data-testid="toolbar-btn-${id}"]`).click();
	await page.waitForFunction((t) => window.__waffle?.getState()?.activeTool === t, id, { timeout: 3000 });
}
const clickPoint = (p) => activateTool(p, 'point');

/** Arm a point by hovering it after a far waypoint (so no other point is crossed). */
async function arm(page, ox, oy, wpx = 0, wpy = 260) {
	await moveTo(page, wpx, wpy);
	await moveTo(page, ox, oy);
	await page.waitForTimeout(120);
}

test.describe('snap inference — ADVERSARIAL', () => {
	// ---- Target 1: armed source deleted (undo) — no dangling align/constraint.
	test('AS1: undoing an armed source drops it (no align, no dangling constraint)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);

		await clickAt(page, -60, 40);
		await page.waitForTimeout(150);
		const P = (await pointPos(page))[0];
		expect(P, 'source P placed').toBeTruthy();

		await arm(page, -60, 40);
		expect((await indicator(page))?.type, 'hovering P arms it (coincident)').toBe('coincident');
		expect((await inferSources(page)).some((s) => s.id === P.id), 'P armed').toBe(true);

		// Undo P's creation → P is gone from the sketch.
		await page.evaluate(() => window.__waffle.undo());
		await page.waitForTimeout(250);
		expect((await pointPos(page)).length, 'P removed by undo').toBe(0);
		expect((await inferSources(page)).length, 'armed sources drop the deleted point').toBe(0);

		// Enter where P's horizontal band was → no align (source gone).
		await arm(page, 60, 44);
		expect((await indicator(page))?.type, 'no align to a deleted source').not.toBe('align-h');

		// Click there → a fresh point, but NO dangling HorizontalPoints.
		await clickAt(page, 60, 44);
		await page.waitForTimeout(200);
		expect((await cbt(page, 'HorizontalPoints')).length, 'no dangling point-pair constraint').toBe(0);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 2: tool switch + sketch exit clear the LRU.
	test('AS2: tool switch and sketch exit clear armed sources', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);

		await clickAt(page, -60, 40);
		await page.waitForTimeout(150);
		await arm(page, -60, 40);
		expect((await inferSources(page)).length, 'armed under point tool').toBe(1);

		// point → line clears.
		await activateTool(page, 'line');
		expect((await inferSources(page)).length, 'tool switch clears the LRU').toBe(0);
		await arm(page, 60, 44);
		expect((await indicator(page))?.type, 'no align after the LRU is cleared').not.toBe('align-h');

		// Re-arm under line, then line → circle → line, each switch clears.
		await arm(page, -60, 40);
		expect((await inferSources(page)).length, 're-armed under line tool').toBe(1);
		await activateTool(page, 'circle');
		expect((await inferSources(page)).length, 'line→circle clears').toBe(0);
		await activateTool(page, 'line');
		expect((await inferSources(page)).length, 'circle→line clears').toBe(0);

		// Sketch exit clears too.
		await arm(page, -60, 40);
		expect((await inferSources(page)).length, 're-armed before exit').toBe(1);
		await page.evaluate(() => window.__waffle.exitSketch());
		await page.waitForTimeout(150);
		await clickSketch(page);
		expect((await inferSources(page)).length, 'sketch exit clears armed sources').toBe(0);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 3 (I7): the align band tracks SCREEN px across a zoom.
	test('AS3: alignment band stays screen-calibrated after a zoom', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);

		await clickAt(page, -60, 40);
		await page.waitForTimeout(150);
		const P = (await pointPos(page))[0];
		await arm(page, -60, 40);
		expect((await inferSources(page)).length, 'P armed').toBe(1);

		const ps0 = await pxSize(page);
		const bounds = await page.evaluate(() => {
			const c = document.querySelector('canvas').getBoundingClientRect();
			return { cx: c.left + c.width / 2, cy: c.top + c.height / 2 };
		});
		// Zoom in (centered) with a real wheel; direction-agnostic — we only need
		// the scale to CHANGE, then verify the band is still ~6px on screen.
		await page.mouse.move(bounds.cx, bounds.cy);
		await page.mouse.wheel(0, -240);
		await page.waitForTimeout(200);
		// sketchPixelSize is refreshed on pointer events (SketchInteraction), so
		// nudge the pointer once post-zoom before reading the new scale.
		await moveTo(page, 0, -180);
		const ps1 = await pxSize(page);
		expect(ps1, 'zoom changed the sketch pixel scale').not.toBeCloseTo(ps0, 12);

		// P's current screen offset after zoom.
		const off = await screenOff(page, P.x, P.y);
		expect(Math.abs(off.x) < 600 && Math.abs(off.y) < 340, 'P still on-canvas after zoom').toBe(true);

		// 4px off P's (screen) horizontal axis, x far → still align-h.
		await moveTo(page, off.x + 130, off.y);
		await moveTo(page, off.x + 130, off.y + 4);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, '4px off axis after zoom → align-h (band is screen-px)').toBe('align-h');

		// 24px off the axis → band must NOT reach (it did not become huge in world).
		await moveTo(page, off.x + 130, off.y - 200);
		await moveTo(page, off.x + 130, off.y + 24);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, '24px off axis → NOT align (band not sticky/huge)').not.toBe('align-h');

		expectNoAnyCrash(crashes);
	});

	// ---- Target 4: over-constraint avoidance — when the align band crosses an
	// existing point, priority-1 coincident wins; NO point-pair constraint is
	// emitted (spec failure mode), so align cannot silently over-constrain a
	// point that is already constrained. (A genuine solver contradiction is not
	// reachable through the align UI alone — see report — because align only ever
	// creates a fresh free point and never re-constrains an existing one.)
	test('AS4: align yields to a point in the band (no over-constraining point-pair)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);

		await clickAt(page, -60, 40); // A (armed source)
		await page.waitForTimeout(120);
		await moveTo(page, 0, 240);
		await clickAt(page, 60, 40); // R, same y as A, in A's future h-band
		await page.waitForTimeout(120);
		expect((await pointPos(page)).length, 'A and R placed').toBe(2);

		const A = (await pointPos(page))[0];
		await arm(page, -60, 40); // arm A
		expect((await inferSources(page)).some((s) => s.id === A.id), 'A armed').toBe(true);

		// Hover R (60,40): in A's horizontal band AND on top of R → coincident wins.
		await arm(page, 60, 40);
		expect((await indicator(page))?.type, 'point in band → coincident (priority-1) wins over align').toBe('coincident');

		const before = (await cbt(page, 'HorizontalPoints')).length;
		await clickAt(page, 60, 40);
		await page.waitForTimeout(250);
		expect((await cbt(page, 'HorizontalPoints')).length - before, 'no point-pair constraint when coincident wins').toBe(0);
		expect((await pointPos(page)).length, 'reused R — no new point').toBe(2);
		expect((await overConstrained(page)).length, 'no spurious over-constraint introduced').toBe(0);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 5: on-entity on a CIRCLE emits OnEntity and is parametric.
	test('AS5: on-entity on a circle emits OnEntity and the point tracks the circle', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);

		// Draw a circle: center at origin (snaps origin), radius via a second click.
		await activateTool(page, 'circle');
		await clickAt(page, 0, 0);
		await page.waitForTimeout(120);
		await clickAt(page, 90, 0);
		await page.waitForTimeout(200);
		const circle = await page.evaluate(() => window.__waffle.getEntities().find((e) => e.type === 'Circle') ?? null);
		expect(circle, 'circle created').toBeTruthy();

		// Point tool → hover the circumference at ~45° (OFF the quadrant points,
		// which are priority-1 snaps at 0/90/180/270°). Radius ≈ 90px → (64,64) is
		// on the circle (dist ≈ 90.5px) and clear of every quadrant marker.
		await clickPoint(page);
		await moveTo(page, 0, 240);
		await moveTo(page, 64, 64);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, 'cursor on the circle (off-quadrant) → on-entity').toBe('on-entity');

		const before = await pointPos(page);
		await clickAt(page, 64, 64);
		await page.waitForTimeout(250);
		expect((await cbt(page, 'OnEntity')).length, 'placing on a circle emits OnEntity (parametric)').toBeGreaterThanOrEqual(1);
		const after = await pointPos(page);
		const q = after.find((p) => !before.some((b) => b.id === p.id));
		expect(q, 'a new point placed on the circle').toBeTruthy();

		// Move the circle by dragging its center; Q must stay on the circumference.
		const ps = await pxSize(page);
		const geom = await page.evaluate(() => {
			const c = window.__waffle.getEntities().find((e) => e.type === 'Circle');
			return { centerId: c.center_id, radius: c.radius };
		});
		await page.evaluate(({ id, dx, dy }) => {
			const pos = window.__waffle.getPositions().get(id);
			window.__waffle.dragSketchPoint(id, pos.x + dx, pos.y + dy);
			window.__waffle.finalizeDrag();
		}, { id: geom.centerId, dx: 30 * ps, dy: 20 * ps });
		await page.waitForTimeout(400);

		const distErr = await page.evaluate(({ qid, cid, r }) => {
			const pos = window.__waffle.getPositions();
			const c = pos.get(cid), qq = pos.get(qid);
			return Math.abs(Math.hypot(qq.x - c.x, qq.y - c.y) - r);
		}, { qid: q.id, cid: geom.centerId, r: geom.radius });
		expect(distErr, 'Q stays on the circle after the circle is moved (OnEntity parametric on a circle)').toBeLessThan(3 * ps);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 6: non-line placement paths emit the align constraint.
	test('AS6a: rectangle FIRST corner in an armed band emits HorizontalPoints', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);
		await clickAt(page, -60, 40); // source A
		await page.waitForTimeout(120);

		await activateTool(page, 'rectangle'); // clears LRU; re-arm A under rectangle
		await arm(page, -60, 40);
		expect((await inferSources(page)).length, 'A armed under rectangle tool').toBe(1);

		await arm(page, 60, 44); // A's horizontal band
		expect((await indicator(page))?.type, 'first corner shows align-h').toBe('align-h');
		const before = (await cbt(page, 'HorizontalPoints')).length;
		await clickAt(page, 60, 44); // place first corner
		await page.waitForTimeout(200);
		expect((await cbt(page, 'HorizontalPoints')).length - before, 'rectangle first corner emits HorizontalPoints').toBeGreaterThanOrEqual(1);

		await clickAt(page, 120, 120); // finish the rectangle
		await page.waitForTimeout(150);
		expectNoAnyCrash(crashes);
	});

	test('AS6b: circle CENTER in an armed band emits HorizontalPoints', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);
		await clickAt(page, -60, 40); // source A
		await page.waitForTimeout(120);

		await activateTool(page, 'circle');
		await arm(page, -60, 40);
		expect((await inferSources(page)).length, 'A armed under circle tool').toBe(1);

		await arm(page, 60, 44);
		expect((await indicator(page))?.type, 'circle center shows align-h').toBe('align-h');
		const before = (await cbt(page, 'HorizontalPoints')).length;
		await clickAt(page, 60, 44); // place center
		await page.waitForTimeout(200);
		expect((await cbt(page, 'HorizontalPoints')).length - before, 'circle center emits HorizontalPoints').toBeGreaterThanOrEqual(1);

		await clickAt(page, 120, 44); // radius, finish
		await page.waitForTimeout(150);
		expectNoAnyCrash(crashes);
	});

	// FINDING PROBE: the rectangle's OPPOSITE (finalizing) corner. finalizeRectangle
	// builds corners via createRectangleEdges, which does NOT route through
	// applyPointSnapConstraints — so an align on the second corner snaps the
	// POSITION but drops the constraint. Spec: "every placement path handles the
	// templates through ONE shared helper." Asserts the spec intent → red = finding.
	test('AS6c: rectangle OPPOSITE corner in an armed band emits HorizontalPoints', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);
		await clickAt(page, -60, -80); // source A (low), its h-axis is y=-80
		await page.waitForTimeout(120);

		await activateTool(page, 'rectangle');
		await arm(page, -60, -80);
		expect((await inferSources(page)).length, 'A armed').toBe(1);

		await clickAt(page, -40, 60); // FIRST corner in free space (not near A's axis)
		await page.waitForTimeout(150);

		// Move the opposite corner into A's horizontal band (screen y ≈ -80).
		await moveTo(page, 0, 240);
		await moveTo(page, 60, -76);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, 'opposite corner shows align-h').toBe('align-h');

		const before = (await cbt(page, 'HorizontalPoints')).length;
		await clickAt(page, 60, -76); // finalize the rectangle on the aligned corner
		await page.waitForTimeout(250);
		expect(
			(await cbt(page, 'HorizontalPoints')).length - before,
			'rectangle opposite corner emits HorizontalPoints (shared normalizer)'
		).toBeGreaterThanOrEqual(1);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 7: both bands from TWO different sources → both constraints,
	// each bound to the CORRECT source.
	test('AS7: two-source both-bands emits H from A and V from B with correct sources', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);

		await clickAt(page, -80, 30); // A → horizontal axis y=30
		await page.waitForTimeout(120);
		await moveTo(page, 0, 240);
		await clickAt(page, 50, -70); // B → vertical axis x=50
		await page.waitForTimeout(120);
		const pts = await pointPos(page);
		const A = pts[0], B = pts[1];

		await arm(page, -80, 30);
		await arm(page, 50, -70);

		// (B.x screen, A.y screen) = (50, 30): A's h-band AND B's v-band.
		await moveTo(page, 0, 240);
		await moveTo(page, 50, 30);
		await page.waitForTimeout(180);
		const ind = await indicator(page);
		expect(['align-h', 'align-v'], 'combined band shows an align indicator').toContain(ind?.type);
		expect(ind?.hSource?.id, 'h-source is A').toBe(A.id);
		expect(ind?.vSource?.id, 'v-source is B').toBe(B.id);

		const before = await pointPos(page);
		await clickAt(page, 50, 30);
		await page.waitForTimeout(250);

		const hp = await cbt(page, 'HorizontalPoints');
		const vp = await cbt(page, 'VerticalPoints');
		expect(hp.some((c) => c.point_a === A.id), 'HorizontalPoints bound to A').toBe(true);
		expect(vp.some((c) => c.point_a === B.id), 'VerticalPoints bound to B').toBe(true);

		const ps = await pxSize(page);
		const q = (await pointPos(page)).find((p) => !before.some((b) => b.id === p.id));
		expect(Math.abs(q.y - A.y), 'solved y equals A.y').toBeLessThan(2 * ps);
		expect(Math.abs(q.x - B.x), 'solved x equals B.x').toBeLessThan(2 * ps);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 8: fromPointId self-suppression — drawing FROM an armed point,
	// near its own axis, yields segment-H (one constraint), not a duplicate.
	test('AS8: drawing from an armed point gives segment-H, not a duplicate align', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);
		await clickAt(page, -60, 40); // P
		await page.waitForTimeout(120);

		const P = (await pointPos(page))[0];
		await activateTool(page, 'line');
		await arm(page, -60, 40); // arm P under the line tool
		expect((await inferSources(page)).some((s) => s.id === P.id), 'P armed').toBe(true);

		// Start the line AT P (fromPointId := P).
		await clickAt(page, -60, 40);
		await page.waitForTimeout(150);

		// Move horizontally from P: P is armed AND is fromPointId → align suppressed,
		// segment-H covers it. Indicator must be 'horizontal', not 'align-h'.
		await moveTo(page, 60, 40);
		await page.waitForTimeout(150);
		expect((await indicator(page))?.type, 'from-point on its own axis → segment horizontal (suppressed align)').toBe('horizontal');

		await clickAt(page, 60, 40); // finalize the line
		await page.waitForTimeout(250);
		expect((await cbt(page, 'Horizontal')).length, 'exactly the segment Horizontal').toBeGreaterThanOrEqual(1);
		expect((await cbt(page, 'HorizontalPoints')).length, 'no duplicate HorizontalPoints on the same pair').toBe(0);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 9: LRU stress — refresh the oldest, then arm a 4th; the
	// refreshed one survives and the true-oldest is evicted (LRU=3).
	test('AS9: refreshing the oldest source keeps it; the true-oldest is evicted', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickPoint(page);

		const P = { A: [-100, 20], B: [-40, -40], C: [40, 60], D: [100, -20] };
		for (const [x, y] of Object.values(P)) {
			await moveTo(page, 0, 250);
			await clickAt(page, x, y);
			await page.waitForTimeout(100);
		}
		// Resolve ids by placement order (A,B,C,D are pts[0..3]).
		const [A, B, C, D] = await pointPos(page);

		// Arm A, B, C (skip D).
		for (const [x, y] of [P.A, P.B, P.C]) {
			await arm(page, x, y, 0, 250);
			expect((await indicator(page))?.type, `armed (${x},${y})`).toBe('coincident');
		}
		// Refresh the oldest (A).
		await arm(page, ...P.A, 0, 250);
		// Arm a 4th (D).
		await arm(page, ...P.D, 0, 250);

		const ids = (await inferSources(page)).map((s) => s.id);
		expect(ids.length, 'LRU capped at 3').toBe(3);
		expect(ids.includes(D.id), 'newest D present').toBe(true);
		expect(ids.includes(A.id), 'refreshed A survived').toBe(true);
		expect(ids.includes(C.id), 'C present').toBe(true);
		expect(ids.includes(B.id), 'true-oldest B evicted').toBe(false);
		expect(ids, 'order most-recent first [D, A, C]').toEqual([D.id, A.id, C.id]);

		expectNoAnyCrash(crashes);
	});

	// ---- Target 10: priority sandwich — on-entity (priority 2) outranks align
	// (priority 3) when the band crosses a line the cursor is on.
	test('AS10: on-entity outranks alignment when the align band lies on a line', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);
		await clickLine(page);

		// Host VERTICAL line at x=60; its interior at (60,40) is on-entity.
		await drawLine(page, 60, -120, 60, 120);
		await page.waitForTimeout(200);

		await clickPoint(page);
		const beforeP = await pointPos(page);
		await clickAt(page, -60, 40); // source P → horizontal axis y=40
		await page.waitForTimeout(120);
		const P = (await pointPos(page)).find((p) => !beforeP.some((b) => b.id === p.id));
		await arm(page, -60, 40);
		expect((await inferSources(page)).some((s) => s.id === P.id), 'P armed').toBe(true);

		// (60,40): on the host line (on-entity ≤5px) AND in P's h-band (align-h).
		await moveTo(page, 0, 240);
		await moveTo(page, 60, 40);
		await page.waitForTimeout(180);
		expect((await indicator(page))?.type, 'on-entity (pri-2) outranks align-h (pri-3) on a line').toBe('on-entity');

		// Control: same band, off the line (free space) → align-h wins.
		await moveTo(page, 0, 240);
		await moveTo(page, 0, 40);
		await page.waitForTimeout(180);
		expect((await indicator(page))?.type, 'off the line, still in P band → align-h').toBe('align-h');

		expectNoAnyCrash(crashes);
	});
});
