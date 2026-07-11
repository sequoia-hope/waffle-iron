/**
 * Cycle 2 — select-first projection & picking fixes.
 * Contract: specs/projected_sketch_geometry.md, section
 * "Cycle 2 (2026-07-05): Picking & flow fixes — select-first projection",
 * oracles O1–O6 / invariants I1–I6.
 *
 * REAL POINTER EVENTS ONLY. These tests never call __waffle.projectVertex/
 * projectEdge/projectFace (those are the Cycle 1 shortcuts, exercised by
 * projection.spec.js). Body geometry is picked by moving/clicking the real
 * mouse at screen positions derived from the built box's world coordinates
 * through the worldToScreen helper (or canvas center for the front face, whose
 * raycast unambiguously resolves the front-most face — the same pattern as
 * face-to-feature.spec.js).
 *
 * Expected RED on current code: O1, O2, O3, O6 (F1 gate + F2 occlusion + the
 * unimplemented select-first Proj action). O4 and O5 are controls guarding
 * current-behavior invariants (I5, I1) and may pass today.
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
const sketchHover = (page) => page.evaluate(() => window.__waffle.getSketchHover());
const sketchSelection = (page) => page.evaluate(() => window.__waffle.getSketchSelection());
const resetHover = (page) => page.evaluate(() => window.__waffle.setHoveredRef(null));
const clearSelection = (page) => page.evaluate(() => window.__waffle.clearSelection());

const countType = (ents, type) => ents.filter((e) => e.type === type).length;
const refKind = (r) => r?.kind?.type ?? null;

/**
 * Deterministic pre-hover at a world point. Body hover is driven off the
 * pixel-keyed hover arbitration (proposeHoverRef), and the CadModel face
 * proposal arrives via the Threlte event cycle — a single jump-move can be read
 * before that proposal lands under parallel load. A distinct priming move from
 * an offset pixel guarantees a fresh pointermove event onto the target (defeats
 * coalescing), and the settle wait lets all overlays propose before we read the
 * hover. Mirrors the A6 hardening in projection-select-first-adversarial.spec.js.
 * @param {import('@playwright/test').Page} page
 * @param {number[]} world
 * @returns {Promise<{x:number, y:number}>} the target screen pixel
 */
async function settleHoverAtWorld(page, world) {
	const s = await worldToScreen(page, world);
	await page.mouse.move(s.x - 15, s.y - 15);
	await page.waitForTimeout(40);
	await page.mouse.move(s.x, s.y);
	await page.waitForTimeout(150);
	return s;
}

/**
 * Build an axis-aligned box (rect on front plane → extrude 20), then re-enter a
 * fresh sketch on the front plane so the body is in the straight-on view.
 * Returns world-space pick targets derived from the body's real AABB.
 */
async function buildBoxAndReSketch(waffle) {
	const page = waffle.page;

	await clickSketch(page, 'front');
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	const depth = page.locator('[data-testid="extrude-depth"]');
	if (await depth.isVisible()) await depth.fill('20');
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);

	const aabb = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
	expect(aabb, 'box has a bounding box').not.toBeNull();

	// Re-enter a fresh sketch on the front plane — body now in the straight-on view.
	await clickSketch(page, 'front');

	const [mnx, mny, mnz] = aabb.min;
	const [mxx, mxy, mxz] = aabb.max;
	// Near-camera z is irrelevant to screen XY in the straight-on view; use max z.
	return {
		aabb,
		// Top-left-front corner (a real box vertex).
		cornerWorld: [mnx, mxy, mxz],
		// Midpoint of the top-front edge (a real box edge, not near any corner).
		edgeMidWorld: [(mnx + mxx) / 2, mxy, mxz],
		// Center of the front face (a known interior point → deterministically a Face).
		faceCenterWorld: [(mnx + mxx) / 2, (mny + mxy) / 2, mxz],
	};
}

test.describe('Cycle 2 — select-first projection (real pointer)', () => {
	// ---- Infra sanity (NOT an oracle): proves worldToScreen maps world points
	// to the correct screen pixels, via the known-good project-tool face picking
	// path (face hover works today per the Cycle-2 diagnosis). If this goes red,
	// the projection math is wrong and the O1/O6 reds below are not trustworthy.
	// Expected GREEN. (The exact kind AT A CORNER is F3-dependent and belongs to
	// O1, so this check only asserts the corner lands ON the body, not Vertex.)
	test('infra: worldToScreen maps world points to the rendered geometry', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);

		await page.locator(PROJ_BTN).click();
		await page.waitForFunction(() => window.__waffle.isProjectToolActive() === true, { timeout: 3000 });

		// A known interior world point (front-face center) → deterministically a
		// Face, and its screen mapping lands near the canvas center.
		await resetHover(page);
		const fc = await settleHoverAtWorld(page, t.faceCenterWorld);
		expect(refKind(await hoveredRef(page)), 'face-center world point → Face').toBe('Face');
		const bounds = await getCanvasBounds(page);
		expect(
			Math.hypot(fc.x - bounds.centerX, fc.y - bounds.centerY),
			'front-face center maps near canvas center'
		).toBeLessThan(150);

		// The corner world point also lands on the body (exact kind is F3-dependent).
		await resetHover(page);
		await settleHoverAtWorld(page, t.cornerWorld);
		expect(
			['Vertex', 'Edge', 'Face'],
			'corner world point hits the body'
		).toContain(refKind(await hoveredRef(page)));

		expectNoAnyCrash(crashes);
	});

	// ---- O1: Select tool → real-hover vertex / edge / face → matching kind.
	// Deterministic vertex priority at a corner from two approach paths (I2, I3).
	test('O1: Select-tool body hover returns the matching ref kind', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);

		// Vertex at a corner.
		await resetHover(page);
		await moveToWorld(page, t.cornerWorld);
		await page.waitForTimeout(120);
		expect(refKind(await hoveredRef(page)), 'corner → Vertex').toBe('Vertex');

		// Edge at the top-front edge midpoint.
		await resetHover(page);
		await moveToWorld(page, t.edgeMidWorld);
		await page.waitForTimeout(120);
		expect(refKind(await hoveredRef(page)), 'edge midpoint → Edge').toBe('Edge');

		// Face at canvas center.
		await resetHover(page);
		const bounds = await getCanvasBounds(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(120);
		expect(refKind(await hoveredRef(page)), 'center → Face').toBe('Face');

		// I3 — deterministic Vertex priority at the corner regardless of approach.
		const corner = await worldToScreen(page, t.cornerWorld);
		for (const [dx, dy] of [[-40, -40], [50, 55]]) {
			await resetHover(page);
			await page.mouse.move(corner.x + dx, corner.y + dy);
			await page.waitForTimeout(60);
			await page.mouse.move(corner.x, corner.y);
			await page.waitForTimeout(120);
			expect(refKind(await hoveredRef(page)), `corner from (${dx},${dy}) → Vertex`).toBe('Vertex');
		}

		expectNoAnyCrash(crashes);
	});

	// ---- O2: select an edge → Proj → 2 points + 1 line + 2 bindings; cleared.
	test('O2: select body edge then Proj projects it (2 pts + 1 line)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const before = await entities(page);
		const bBefore = (await bindings(page)).length;

		// Real-click the top-front edge midpoint to select the body edge.
		const s = await worldToScreen(page, t.edgeMidWorld);
		await page.mouse.click(s.x, s.y);
		await page.waitForTimeout(150);

		const sel = await selectedRefs(page);
		expect(sel.length, 'one body ref selected').toBe(1);
		expect(refKind(sel[0]), 'selected ref is an Edge').toBe('Edge');

		// Click the Proj toolbar button → project the selected edge.
		await page.locator(PROJ_BTN).click();
		await page.waitForTimeout(250);

		const after = await entities(page);
		expect(countType(after, 'Point') - countType(before, 'Point'), '+2 projected points').toBe(2);
		expect(countType(after, 'Line') - countType(before, 'Line'), '+1 projected line').toBe(1);
		expect((await bindings(page)).length - bBefore, '+2 bindings').toBe(2);
		expect((await selectedRefs(page)).length, 'selection cleared').toBe(0);
		expect(await getActiveTool(page), 'tool stays select').toBe('select');

		expectNoAnyCrash(crashes);
	});

	// ---- O3: face → 4 pts + 4 lines + 4 bindings; vertex → 1 pt + 1 binding.
	test('O3a: select body face then Proj projects its boundary (4 pts + 4 lines)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const before = await entities(page);
		const bBefore = (await bindings(page)).length;

		// Real-click the front face at canvas center.
		await clickAt(page, 0, 0);
		const sel = await selectedRefs(page);
		expect(sel.length, 'one body ref selected').toBe(1);
		expect(refKind(sel[0]), 'selected ref is a Face').toBe('Face');

		await page.locator(PROJ_BTN).click();
		await page.waitForTimeout(250);

		const after = await entities(page);
		expect(countType(after, 'Point') - countType(before, 'Point'), '+4 corner points').toBe(4);
		expect(countType(after, 'Line') - countType(before, 'Line'), '+4 construction lines').toBe(4);
		expect((await bindings(page)).length - bBefore, '+4 bindings').toBe(4);
		expect((await selectedRefs(page)).length, 'selection cleared').toBe(0);
		expect(await getActiveTool(page), 'tool stays select').toBe('select');

		expectNoAnyCrash(crashes);
	});

	test('O3b: select body vertex then Proj projects one point (1 pt + 1 binding)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickSelect(page);
		await clearSelection(page);

		const before = await entities(page);
		const bBefore = (await bindings(page)).length;

		const s = await worldToScreen(page, t.cornerWorld);
		await page.mouse.click(s.x, s.y);
		await page.waitForTimeout(150);

		const sel = await selectedRefs(page);
		expect(sel.length, 'one body ref selected').toBe(1);
		expect(refKind(sel[0]), 'selected ref is a Vertex').toBe('Vertex');

		await page.locator(PROJ_BTN).click();
		await page.waitForTimeout(250);

		const after = await entities(page);
		expect(countType(after, 'Point') - countType(before, 'Point'), '+1 projected point').toBe(1);
		expect((await bindings(page)).length - bBefore, '+1 binding').toBe(1);
		expect((await selectedRefs(page)).length, 'selection cleared').toBe(0);
		expect(await getActiveTool(page), 'tool stays select').toBe('select');

		expectNoAnyCrash(crashes);
	});

	// ---- O4 (control, I5): drawing tool active → body hover gated off.
	test('O4: Line tool active suppresses body hover (control)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);
		await clickLine(page);

		for (const w of [t.cornerWorld, t.edgeMidWorld]) {
			await resetHover(page);
			await moveToWorld(page, w);
			await page.waitForTimeout(100);
			expect(await hoveredRef(page), 'body hover null under Line tool').toBeNull();
		}
		// Also the face center.
		await resetHover(page);
		const bounds = await getCanvasBounds(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(100);
		expect(await hoveredRef(page), 'face hover null under Line tool').toBeNull();

		expectNoAnyCrash(crashes);
	});

	// ---- O5 (control, I1): a sketch entity under the pointer wins over the body.
	test('O5: sketch entity intercepts body pick (control)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await buildBoxAndReSketch(waffle);

		// Draw a sketch line across the front face (through canvas center).
		await clickLine(page);
		await drawLine(page, -40, 0, 40, 0);
		await waitForEntityCount(page, 3, 5000); // 2 endpoints + 1 line

		await clickSelect(page);
		await resetHover(page);
		await clearSelection(page);

		// Hover the line's midpoint (canvas center).
		const bounds = await getCanvasBounds(page);
		await page.mouse.move(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(120);

		expect(await sketchHover(page), 'sketch hover set over the line').not.toBeNull();
		expect(await hoveredRef(page), 'body hover suppressed under a sketch entity').toBeNull();

		// Click → the sketch line is selected; no body ref is selected.
		await page.mouse.click(bounds.centerX, bounds.centerY);
		await page.waitForTimeout(120);
		expect((await sketchSelection(page)).length, 'sketch line selected').toBeGreaterThan(0);
		expect((await selectedRefs(page)).length, 'no body ref selected').toBe(0);

		expectNoAnyCrash(crashes);
	});

	// ---- O6: tool-first edge projection in the straight-on view (F2 fix).
	// Since task #140, a plain click projects the CONNECTED coplanar edge
	// chain (here the front face's closed 4-edge rim); Alt-click keeps the
	// single-edge behavior. The F2 oracle is unchanged: the edge is hoverable
	// straight-on and the click acts on it.
	test('O6: project-tool edge hover+click works straight-on (F2)', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const t = await buildBoxAndReSketch(waffle);

		await clearSelection(page);
		await page.locator(PROJ_BTN).click();
		await page.waitForFunction(() => window.__waffle.isProjectToolActive() === true, { timeout: 3000 });

		const before = await entities(page);
		const bBefore = (await bindings(page)).length;

		// Hover the top-front edge midpoint — previously blocked by the face
		// behind it (F2). After the occlusion fix this registers as an Edge.
		await resetHover(page);
		const s = await moveToWorld(page, t.edgeMidWorld);
		await page.waitForTimeout(120);
		expect(refKind(await hoveredRef(page)), 'straight-on edge hover → Edge').toBe('Edge');

		// Click projects the closed 4-edge rim → 4 corners + 4 lines + 4 bindings.
		await page.mouse.click(s.x, s.y);
		await page.waitForTimeout(250);

		const after = await entities(page);
		expect(countType(after, 'Point') - countType(before, 'Point'), '+4 projected corners').toBe(4);
		expect(countType(after, 'Line') - countType(before, 'Line'), '+4 projected lines').toBe(4);
		expect((await bindings(page)).length - bBefore, '+4 bindings').toBe(4);

		// Alt-click on the same edge chain limits to the single hovered edge
		// (its 2 endpoints dedupe against the already-projected corners is NOT
		// expected — a fresh single-edge projection makes its own 2 points).
		await resetHover(page);
		const s2 = await moveToWorld(page, t.edgeMidWorld);
		await page.waitForTimeout(120);
		await page.keyboard.down('Alt');
		await page.mouse.move(s2.x + 1, s2.y);
		await page.waitForTimeout(100);
		await page.mouse.click(s2.x + 1, s2.y);
		await page.keyboard.up('Alt');
		await page.waitForTimeout(250);
		const after2 = await entities(page);
		expect(countType(after2, 'Line') - countType(after, 'Line'), 'Alt-click: +1 line').toBe(1);

		expectNoAnyCrash(crashes);
	});
});
