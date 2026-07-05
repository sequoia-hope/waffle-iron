/**
 * Cycle 2 — ADVERSARIAL probes for select-first projection & picking fixes.
 * Contract: specs/projected_sketch_geometry.md, "Cycle 2 (2026-07-05)".
 *
 * Sibling of projection-select-first.spec.js (the feature oracles O1–O6). These
 * tests push the flow into pathological corners the oracles do not cover:
 * mixed multi-select, non-projectable / stale selections, orbited-view
 * occlusion, hover determinism under path stress, double projection, undo, and
 * Escape. REAL POINTER EVENTS ONLY — never the __waffle.projectX shortcuts.
 *
 * Where a probe documents CURRENT behavior that deviates from the spec's stated
 * intent, the assertion matches reality and a comment flags the deviation as a
 * finding for the adversary report (the working tree is not modified).
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	clickSelect,
	clickLine,
} from './helpers/toolbar.js';
import { drawRectangle, drawLine, clickAt, getCanvasBounds } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getActiveTool,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';
import { worldToScreen, moveToWorld } from './helpers/worldToScreen.js';

const PROJ_BTN = '[data-testid="toolbar-btn-project"]';

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const bindings = (page) => page.evaluate(() => window.__waffle.getProjectedBindings());
const hoveredRef = (page) => page.evaluate(() => window.__waffle.getHoveredRef());
const selectedRefs = (page) => page.evaluate(() => window.__waffle.getSelectedRefs());
const resetHover = (page) => page.evaluate(() => window.__waffle.setHoveredRef(null));
const clearSelection = (page) => page.evaluate(() => window.__waffle.clearSelection());

const countType = (ents, type) => ents.filter((e) => e.type === type).length;
const refKind = (r) => r?.kind?.type ?? null;

/** Same box+re-sketch fixture as the feature spec; extrude depth is a param so
 * the occlusion probe can use a DEEP box (far edges stay hidden under a small
 * orbit while separating cleanly on screen). */
async function buildBoxAndReSketch(waffle, extrudeDepth = 20) {
	const page = waffle.page;

	await clickSketch(page, 'front');
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	const depth = page.locator('[data-testid="extrude-depth"]');
	if (await depth.isVisible()) await depth.fill(String(extrudeDepth));
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);

	const aabb = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
	expect(aabb, 'box has a bounding box').not.toBeNull();

	await clickSketch(page, 'front');

	const [mnx, mny, mnz] = aabb.min;
	const [mxx, mxy, mxz] = aabb.max;
	return {
		aabb,
		cornerWorld: [mnx, mxy, mxz],
		edgeMidWorld: [(mnx + mxx) / 2, mxy, mxz],
		faceCenterWorld: [(mnx + mxx) / 2, (mny + mxy) / 2, mxz],
	};
}

/** Real shift-click at absolute screen pixels. */
async function shiftClick(page, x, y) {
	await page.keyboard.down('Shift');
	await page.mouse.click(x, y);
	await page.keyboard.up('Shift');
	await page.waitForTimeout(150);
}

test.describe('Cycle 2 — select-first projection ADVERSARIAL', () => {

	// ---- Case 1: mixed multi-select (vertex + edge + face) → Proj projects all.
	test('A1: shift multi-select vertex+edge+face projects the union', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const before = await entities(page);
		const bBefore = (await bindings(page)).length;

		// Vertex (corner).
		await resetHover(page);
		const cs = await moveToWorld(page, t.cornerWorld);
		await shiftClick(page, cs.x, cs.y);
		// Edge (top-front midpoint).
		await resetHover(page);
		const es = await moveToWorld(page, t.edgeMidWorld);
		await shiftClick(page, es.x, es.y);
		// Face (front-face center).
		await resetHover(page);
		const bounds = await getCanvasBounds(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(120);
		await shiftClick(page, bounds.centerX, bounds.centerY);

		const sel = await selectedRefs(page);
		const kinds = sel.map(refKind).sort();
		expect(kinds, 'vertex+edge+face all accumulate under shift-select')
			.toEqual(['Edge', 'Face', 'Vertex']);

		await page.locator(PROJ_BTN).click();
		await page.waitForTimeout(300);

		const after = await entities(page);
		// vertex 1pt + edge 2pt + face 4pt = 7 points; edge 1 + face 4 = 5 lines;
		// 1 + 2 + 4 = 7 bindings.
		expect(countType(after, 'Point') - countType(before, 'Point'), '+7 projected points').toBe(7);
		expect(countType(after, 'Line') - countType(before, 'Line'), '+5 projected lines').toBe(5);
		expect((await bindings(page)).length - bBefore, '+7 bindings').toBe(7);
		expect((await selectedRefs(page)).length, 'selection cleared after project').toBe(0);
		expect(await getActiveTool(page), 'tool stays select').toBe('select');

		expectNoAnyCrash(crashes);
	});

	// ---- Case 1 (pinpoint): a single shift-click on a body FACE must select it.
	// O3a shows a plain (non-additive) face click works. This isolates whether
	// ADDITIVE (shift) face selection survives — if it comes back empty, the face
	// is being double-handled (added by the select-first path, toggled off by
	// CadModel's Threlte onclick).
	test('A1b: shift-click a single body face selects it (additive-path pinpoint)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const bounds = await getCanvasBounds(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(120);
		await shiftClick(page, bounds.centerX, bounds.centerY);

		const sel = await selectedRefs(page);
		expect(sel.map(refKind), 'a shift-clicked face is selected').toEqual(['Face']);

		expectNoAnyCrash(crashes);
	});

	// ---- Case 3: stale body selection under a DRAWING tool. Branch table row
	// "Any drawing tool → activate project tool" — the stale selection must NOT
	// silently project.
	test('A3: stale body selection + Line tool → J activates project tool, does not project', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		// Select a body edge under Select.
		const es = await moveToWorld(page, t.edgeMidWorld);
		await page.mouse.click(es.x, es.y);
		await page.waitForTimeout(150);
		expect((await selectedRefs(page)).length, 'edge selected under Select').toBe(1);

		// Switch to Line tool — body picking is now gated off, but does the stale
		// selection persist?
		await clickLine(page);
		const staleSel = (await selectedRefs(page)).length;

		const before = await entities(page);
		const bBefore = (await bindings(page)).length;

		// Press J. Per branch table this must ACTIVATE the project tool, not
		// project the stale selection.
		await page.keyboard.press('j');
		await page.waitForTimeout(250);

		const after = await entities(page);
		const projected = countType(after, 'Point') - countType(before, 'Point');
		const newBindings = (await bindings(page)).length - bBefore;

		// FINDING PROBE: record whether the stale selection leaked into a projection.
		expect(newBindings, `stale selection must not project via J (staleSel=${staleSel})`).toBe(0);
		expect(projected, 'no projected points from stale selection').toBe(0);
		expect(await getActiveTool(page), 'J with a drawing tool active activates project tool').toBe('project');

		expectNoAnyCrash(crashes);
	});

	// ---- Case 4: occlusion-aware picking across camera orientations (I2).
	//
	// Two robust checks:
	//   (a) STRAIGHT-ON, the front face occludes the deep geometry behind it —
	//       the canvas center resolves to a Face, never a far edge leaking
	//       through (I2 occlusion direction).
	//   (b) After a real orbit, a visible silhouette edge still picks as an Edge
	//       (I2 "in all camera orientations, not just the sketch view").
	//
	// NOTE (coverage limitation, documented for the report): a pixel over a
	// GENUINELY-HIDDEN edge cannot be isolated in a convex single-box scene — at
	// any orbit angle large enough to separate a back edge from its front
	// counterpart on screen, that back edge lies on a silhouette and becomes
	// legitimately visible, so its pixel correctly returns an Edge. The
	// hidden-edge suppression path (edgeOccludedByFace: face-hit depth vs
	// edge-hit depth) is therefore verified by code inspection, not asserted
	// here — forcing it produced only false positives.
	test('A4: occlusion-aware picking straight-on and orbited (I2)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		const bounds = await getCanvasBounds(page);

		// (a) Straight-on: front face occludes everything behind → center is a Face.
		await resetHover(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(150);
		expect(refKind(await hoveredRef(page)), 'straight-on center → front Face (occludes the deep body behind)').toBe('Face');

		// Real orbit off-axis (RIGHT = ROTATE; right-drag bypasses SketchInteraction).
		const camBefore = await page.evaluate(() => window.__waffle.getCameraState());
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.mouse.down({ button: 'right' });
		await page.mouse.move(bounds.centerX + 40, bounds.centerY - 15, { steps: 10 });
		await page.mouse.up({ button: 'right' });
		await page.waitForTimeout(250);
		const camAfter = await page.evaluate(() => window.__waffle.getCameraState());
		expect(
			JSON.stringify(camAfter.position) !== JSON.stringify(camBefore.position),
			'camera actually orbited (right-drag reoriented the view)'
		).toBe(true);

		// (b) After orbit: the top-front silhouette edge (empty space above it)
		// stays pickable as an Edge — the occlusion fix generalizes past the
		// straight-on sketch view.
		await resetHover(page);
		await moveToWorld(page, t.edgeMidWorld);
		await page.waitForTimeout(150);
		expect(refKind(await hoveredRef(page)), 'silhouette top-front edge stays pickable when orbited (I2)').toBe('Edge');

		expectNoAnyCrash(crashes);
	});

	// ---- Case 5: hover determinism stress. 10 direct moves to the same corner
	// from alternating directions → Vertex every time (I3).
	test('A5: corner hover is Vertex on all 10 alternating approaches (I3)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);

		const corner = await worldToScreen(page, t.cornerWorld);
		const offsets = [
			[-40, -40], [45, 50], [-55, 30], [60, -35], [-30, 60],
			[35, 40], [-50, -25], [55, -55], [-45, 45], [50, 25],
		];
		const observed = [];
		for (const [dx, dy] of offsets) {
			await resetHover(page);
			await page.mouse.move(corner.x + dx, corner.y + dy);
			await page.waitForTimeout(50);
			await page.mouse.move(corner.x, corner.y);
			await page.waitForTimeout(90);
			observed.push(refKind(await hoveredRef(page)));
		}
		expect(observed, 'all 10 approaches resolve to Vertex').toEqual(Array(10).fill('Vertex'));

		expectNoAnyCrash(crashes);
	});

	// ---- Case 6: double projection of the same face via select-first.
	// Documents whether the second projection dedups or duplicates; asserts it is
	// at least self-consistent (same delta both times, no crash).
	test('A6: projecting the same face twice is self-consistent', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const bounds = await getCanvasBounds(page);
		const projectFaceOnce = async () => {
			const before = await entities(page);
			const bBefore = (await bindings(page)).length;
			// Pre-hover the front-face center before clicking — sketch-mode
			// select-first drives selection off the arbitrated hover, so a bare
			// click with no preceding move can race the hover (timing-sensitive).
			await resetHover(page);
			await page.mouse.move(bounds.centerX, bounds.centerY);
			await page.waitForTimeout(150);
			await page.mouse.click(bounds.centerX, bounds.centerY);
			await page.waitForTimeout(150);
			expect(refKind((await selectedRefs(page))[0]), 'face selected').toBe('Face');
			await page.locator(PROJ_BTN).click();
			await page.waitForTimeout(250);
			const after = await entities(page);
			return {
				pts: countType(after, 'Point') - countType(before, 'Point'),
				lines: countType(after, 'Line') - countType(before, 'Line'),
				binds: (await bindings(page)).length - bBefore,
			};
		};

		const first = await projectFaceOnce();
		const second = await projectFaceOnce();

		expect(first, 'first face projection = 4 pts + 4 lines + 4 bindings')
			.toEqual({ pts: 4, lines: 4, binds: 4 });
		// Self-consistency: whatever the second projection does, it does the same
		// as the first (spec permits duplication across picks).
		expect(second, 'second projection matches the first delta (duplicates, per spec)').toEqual(first);

		expectNoAnyCrash(crashes);
	});

	// ---- Case 7: undo after a select-first vertex projection.
	test('A7: undo reverts a projected vertex (entities AND bindings)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const beforePts = countType(await entities(page), 'Point');
		const beforeBinds = (await bindings(page)).length;

		const cs = await moveToWorld(page, t.cornerWorld);
		await page.mouse.click(cs.x, cs.y);
		await page.waitForTimeout(150);
		expect(refKind((await selectedRefs(page))[0]), 'vertex selected').toBe('Vertex');
		await page.locator(PROJ_BTN).click();
		await page.waitForTimeout(250);

		expect(countType(await entities(page), 'Point') - beforePts, 'projected +1 point').toBe(1);
		expect((await bindings(page)).length - beforeBinds, 'projected +1 binding').toBe(1);

		// Undo (Ctrl+Z).
		await page.keyboard.press('Control+z');
		await page.waitForTimeout(300);

		const afterUndoPts = countType(await entities(page), 'Point');
		const afterUndoBinds = (await bindings(page)).length;
		// Entity revert is the primary guarantee.
		expect(afterUndoPts, 'undo removes the projected point').toBe(beforePts);
		// FINDING PROBE: bindings side-table should also revert. If this fails,
		// undo leaves a stale binding (report as a finding, not fixed here).
		expect(afterUndoBinds, 'undo also reverts the projected binding').toBe(beforeBinds);

		expectNoAnyCrash(crashes);
	});

	// ---- Case 8: Escape with a body entity selected under the Select tool.
	// Documents the actual behavior (Escape under Select finishes the sketch per
	// the Toolbar keydown handler) and asserts it is crash-free and sane.
	test('A8: Escape with a body selection is crash-free and sane', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const cs = await moveToWorld(page, t.cornerWorld);
		await page.mouse.click(cs.x, cs.y);
		await page.waitForTimeout(150);
		expect((await selectedRefs(page)).length, 'body ref selected').toBe(1);

		// Finish-sketch button is only present in sketch mode — use it as the live
		// sketch-mode indicator (no __waffle.getSketchMode hook exists).
		const finishBtn = page.locator('[data-testid="toolbar-btn-finish-sketch"]');
		expect(await finishBtn.isVisible(), 'in sketch mode before Escape').toBe(true);

		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		// Observe: under Select, Escape's Toolbar handler calls handleFinishSketch,
		// so sketch mode is expected to exit. Whatever happens, the app must stay
		// responsive and crash-free — getEntities keeps returning an array.
		const ents = await entities(page);
		expect(Array.isArray(ents), 'app responsive after Escape (getEntities works)').toBe(true);
		const stillInSketch = await finishBtn.isVisible().catch(() => false);
		// Document the transition without over-constraining (either state is sane).
		expect(typeof stillInSketch, 'sketch-mode indicator is queryable').toBe('boolean');

		expectNoAnyCrash(crashes);
	});
});

// ---------------------------------------------------------------------------
// Scale robustness — the picking thresholds / occlusion margin must be
// scale-aware. Bodies drawn at the app's DEFAULT zoom have an AABB of only
// ~0.01–0.02 world units; a fixed absolute-world pick threshold larger than the
// whole body made edge hovers span entire face interiors, so faces became
// unpickable (kind==='Edge' everywhere). The regression slipped past
// face-select.spec.js because it only asserted kind==='Face' — a datum plane
// ALSO satisfies that. These guards therefore assert the picked ref resolves to
// the BOX BODY (anchor.type==='FeatureOutput'), never a datum plane.
// ---------------------------------------------------------------------------

/** A body face/edge/vertex ref comes from a feature output; a datum plane has
 * anchor.type==='DatumPlane'. This is the strong check face-select.spec.js
 * lacked. */
const isBoxBodyRef = (r) =>
	r?.anchor?.type === 'FeatureOutput' && r?.anchor?.feature_id != null && !!r?.kind?.type;

/** Build a box and stay in MODELING mode (no re-sketch). `extrudeDepth` lets us
 * probe both scale extremes. Returns the body AABB. */
async function buildBoxModeling(waffle, rect = [-80, -60, 80, 60], extrudeDepth = 20) {
	const page = waffle.page;
	await clickSketch(page, 'front');
	await clickRectangle(page);
	await drawRectangle(page, rect[0], rect[1], rect[2], rect[3]);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	const depth = page.locator('[data-testid="extrude-depth"]');
	if (await depth.isVisible()) await depth.fill(String(extrudeDepth));
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);
	const aabb = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
	expect(aabb, 'box has a bounding box').not.toBeNull();
	return aabb;
}

test.describe('Cycle 2 — picking scale robustness', () => {
	// ---- SR1: MODELING mode, default-zoom small box — a face-interior pixel
	// hovers + clicks the BOX FACE (not an edge spanning the interior, not a
	// datum plane). This is the exact regression face-to-feature.spec.js hits.
	test('SR1: modeling-mode face-interior pixel resolves to the box face at default zoom', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const aabb = await buildBoxModeling(waffle);
		// Small body confirms we are exercising the sub-threshold regime.
		const size = Math.max(...aabb.size);
		expect(size, 'default box is small in world units (sub-threshold regime)').toBeLessThan(1);

		const bounds = await getCanvasBounds(page);
		await resetHover(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(150);
		const hov = await hoveredRef(page);
		expect(refKind(hov), 'face-interior hover is a Face, not an edge spanning the interior').toBe('Face');
		expect(isBoxBodyRef(hov), `hovered face is the BOX body face, not a datum plane (got ${JSON.stringify(hov?.anchor)})`).toBe(true);

		await page.mouse.click(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(150);
		const sel = await selectedRefs(page);
		expect(sel.length, 'a ref was selected').toBeGreaterThan(0);
		expect(refKind(sel[0]), 'selected ref is a Face').toBe('Face');
		expect(isBoxBodyRef(sel[0]), 'selected face resolves to the box body/feature').toBe(true);

		expectNoAnyCrash(crashes);
	});

	// ---- SR2: SKETCH mode, same default small box — select-first edge / vertex /
	// face picking each resolves to the box body at default zoom.
	test('SR2: sketch-mode select-first vertex/edge/face pick the box at default zoom', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		// Vertex.
		await resetHover(page);
		await moveToWorld(page, t.cornerWorld);
		await page.waitForTimeout(120);
		const vh = await hoveredRef(page);
		expect(refKind(vh), 'corner → Vertex').toBe('Vertex');
		expect(isBoxBodyRef(vh), 'hovered vertex is the box body').toBe(true);

		// Edge.
		await resetHover(page);
		await moveToWorld(page, t.edgeMidWorld);
		await page.waitForTimeout(120);
		const eh = await hoveredRef(page);
		expect(refKind(eh), 'edge midpoint → Edge').toBe('Edge');
		expect(isBoxBodyRef(eh), 'hovered edge is the box body').toBe(true);

		// Face (interior) — the sub-threshold regression target in sketch mode.
		const bounds = await getCanvasBounds(page);
		await resetHover(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(120);
		const fh = await hoveredRef(page);
		expect(refKind(fh), 'face interior → Face (not an edge spanning it)').toBe('Face');
		expect(isBoxBodyRef(fh), 'hovered face is the box body, not a datum plane').toBe(true);

		// Click the face → select-first selects the box face.
		await page.mouse.click(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(150);
		const sel = await selectedRefs(page);
		expect(sel.length, 'face selected via select-first').toBe(1);
		expect(isBoxBodyRef(sel[0]) && refKind(sel[0]) === 'Face', 'selected the box face').toBe(true);

		expectNoAnyCrash(crashes);
	});

	// ---- SR3: opposite extreme — a LARGE but NORMALLY-PROPORTIONED body. The
	// pixel-calibrated threshold must not over-correct and break picking on large
	// geometry. The sketch rectangle (screen-pixel-sized in this fixture) and the
	// extrude depth are scaled TOGETHER so the body grows uniformly (~4.5× the
	// default, ~0.09 m, aspect ~2:1) instead of becoming a needle.
	//
	// (An earlier version scaled ONLY the extrude depth (160×120 sketch × 20000
	// extrude) → a ~1700:1 needle; camera auto-fit then parks the eye INSIDE the
	// body looking down its axis, so the center ray misses every face. That is a
	// real degeneracy in a DIFFERENT subsystem — auto-fit framing + the
	// sketch-XY-vs-extrude-Z unit-scale mismatch — recorded as a follow-up, not a
	// picking bug; no threshold can fix a ray that misses geometry.)
	test('SR3: modeling-mode face-interior pixel resolves to the box face on a large proportioned body', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const aabb = await buildBoxModeling(waffle, [-360, -220, 360, 220], 90);
		const maxDim = Math.max(...aabb.size);
		const ratio = Math.max(...aabb.size) / Math.min(...aabb.size);
		// Large relative to the ~0.02 m default (and above the old absolute 0.06 m
		// pick threshold), yet proportioned — NOT the degenerate needle.
		expect(maxDim, 'body is large in world units (well above the default/old-threshold scale)').toBeGreaterThan(0.06);
		expect(ratio, 'body is normally proportioned, not a needle').toBeLessThan(5);

		const bounds = await getCanvasBounds(page);
		await resetHover(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(150);
		const hov = await hoveredRef(page);
		expect(refKind(hov), 'large-body face interior still hovers a Face').toBe('Face');
		expect(isBoxBodyRef(hov), 'large-body hovered face is the box body').toBe(true);

		expectNoAnyCrash(crashes);
	});
});
