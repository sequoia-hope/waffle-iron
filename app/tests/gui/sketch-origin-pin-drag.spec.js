/**
 * Origin-pin lock + drag-stability regression tests.
 * Specs: specs/pinned_constraint.md, specs/sketch_drag_stability.md.
 *
 * User repro (2026-07-04): two origin-centered centerpoint rectangles,
 * Equal on two adjacent inner edges, drag the inner corner → geometry
 * exploded to 1e8 and the "origin-snapped" center walked away. Root causes:
 * LM null-space runaway (fixed by proximal regularization in sketch-solver)
 * and the origin snap's WhereDragged{x,y} being lowered to Dragged (target
 * dropped, weight 1/20) — now lowered to Pinned{point,x,y} at weight 1.0.
 *
 * Drawing uses real pointer events; the drag itself goes through
 * __waffle.dragSketchPoint (the exact production dragSketchPoint →
 * triggerSolve → sketchSolved path). Equal is applied via
 * __waffle.addSketchConstraint (constraint SETUP, not drawing).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, clickCenterRectangle } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, getEntities, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const getConstraints = (page) => page.evaluate(() => window.__waffle.getConstraints());
const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));

/** Drag a point through a multi-step path via the production drag API. */
async function dragPointAlongPath(page, pointId, path) {
	for (const [x, y] of path) {
		await page.evaluate(
			([id, px, py]) => window.__waffle.dragSketchPoint(id, px, py),
			[pointId, x, y]
		);
		await page.waitForTimeout(60); // let the async solve round-trip land
	}
	await page.evaluate(() => window.__waffle.finalizeDrag());
	await page.waitForTimeout(300);
}

test.describe('origin pin lock during drag', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickCenterRectangle(waffle.page);
	});

	test('origin-snapped center holds through an off-axis corner drag', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Center rectangle centered on the sketch origin (canvas center).
		await drawRectangle(page, 0, 0, 80, 60);
		await waitForEntityCount(page, 11, 5000);

		// Premise guard: the origin snap must have pinned the center.
		const cons = await getConstraints(page);
		const pin = cons.find((c) => c.type === 'WhereDragged' && !c._isDrag);
		expect(pin, 'origin snap should emit a WhereDragged pin').toBeTruthy();
		expect(Math.abs(pin.x)).toBeLessThan(1e-9);
		expect(Math.abs(pin.y)).toBeLessThan(1e-9);

		// Make it a square (Equal on two adjacent edges) — the user repro.
		const entities = await getEntities(page);
		const lines = entities.filter((e) => e.type === 'Line');
		await page.evaluate(
			([a, b]) => window.__waffle.addSketchConstraint({ type: 'Equal', entity_a: a, entity_b: b }),
			[lines[0].id, lines[1].id]
		);
		await page.waitForTimeout(300);

		const points = entities.filter((e) => e.type === 'Point');
		const centerId = points[0].id; // insertion order: [center, p1..p4, mids]
		const cornerId = points[3].id; // p3 = (+hx, +hy) corner
		const pos0 = await getPositions(page);
		const corner0 = pos0[cornerId];

		// Drag the corner along a deliberately off-axis path (the mouse fights
		// the diagonal DOF). The pin must win; the cursor drifts off the point.
		const path = [];
		for (let i = 1; i <= 12; i++) {
			path.push([
				corner0.x + corner0.x * 0.15 * i, // pull outward along +x
				corner0.y + corner0.y * 0.02 * i * Math.sin(i * 0.7), // wiggle y
			]);
		}
		await dragPointAlongPath(page, cornerId, path);

		const pos1 = await getPositions(page);
		const center1 = pos1[centerId];
		const corner1 = pos1[cornerId];
		const centerDrift = Math.hypot(center1.x, center1.y);
		const cornerMove = Math.hypot(corner1.x - corner0.x, corner1.y - corner0.y);

		// The corner must actually move…
		expect(cornerMove).toBeGreaterThan(Math.abs(corner0.x) * 0.5);
		// …while the pinned center stays at the origin (release solve is exact;
		// tolerance is generous vs the old behavior where drift ≈ cornerMove).
		expect(centerDrift).toBeLessThan(Math.abs(corner0.x) * 0.01);

		expectNoAnyCrash(crashes);
	});

	test('two nested center rectangles + equal square: corner drag never explodes', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Inner (drawn first) then outer, both centered on the origin.
		await drawRectangle(page, 0, 0, 80, 60);
		await waitForEntityCount(page, 11, 5000);
		await clickCenterRectangle(page); // tool resets to idle after each rect
		await drawRectangle(page, 0, 0, 110, 90);
		await waitForEntityCount(page, 21, 5000); // +4 pts +4 lines +2 mids (center reused)

		const entities = await getEntities(page);
		const lines = entities.filter((e) => e.type === 'Line');
		// First 4 lines belong to the inner rectangle: l1 = bottom, l2 = right.
		await page.evaluate(
			([a, b]) => window.__waffle.addSketchConstraint({ type: 'Equal', entity_a: a, entity_b: b }),
			[lines[0].id, lines[1].id]
		);
		await page.waitForTimeout(300);

		const points = entities.filter((e) => e.type === 'Point');
		const cornerId = points[3].id; // inner (+hx,+hy) corner
		const pos0 = await getPositions(page);
		const corner0 = pos0[cornerId];
		// Envelope: everything must stay within 10x the initial outer extent
		// (the bug blew coordinates up by 7 orders of magnitude in 2 moves).
		const extent0 = Math.max(
			...Object.values(pos0).flatMap((p) => [Math.abs(p.x), Math.abs(p.y)])
		);

		const path = [];
		for (let i = 1; i <= 12; i++) {
			path.push([
				corner0.x + corner0.x * 0.2 * i,
				corner0.y + corner0.y * 0.05 * i * Math.sin(i * 0.7),
			]);
		}
		await dragPointAlongPath(page, cornerId, path);

		const pos1 = await getPositions(page);
		for (const [id, p] of Object.entries(pos1)) {
			expect(Number.isFinite(p.x) && Number.isFinite(p.y), `point ${id} finite`).toBe(true);
			expect(
				Math.abs(p.x) < extent0 * 10 && Math.abs(p.y) < extent0 * 10,
				`point ${id} bounded, got (${p.x}, ${p.y})`
			).toBe(true);
		}

		expectNoAnyCrash(crashes);
	});
});
