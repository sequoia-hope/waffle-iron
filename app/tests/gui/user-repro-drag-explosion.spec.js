/**
 * REAL-POINTER reproduction of the user's exact report (2026-07-05):
 * two origin-centered centerpoint rectangles, Equal on two adjacent inner
 * edges, grab the inner corner with the MOUSE and drag it around fast.
 * Then Ctrl+Z: the DRAG must be undone (not the Equal constraint) and the
 * camera must return.
 *
 * Unlike sketch-origin-pin-drag.spec.js (API drag via __waffle), this drives
 * tools.js handleSelectTool: hitTest → drag threshold → per-pointermove
 * detectSnaps + dragSketchPoint → pointerup → finalizeDrag +
 * applyDragEndConstraints. Each inner corner is exercised — including the
 * corner SHARED by the two Equal edges.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect, clickCenterRectangle } from './helpers/toolbar.js';
import { drawRectangle, getCanvasBounds } from './helpers/canvas.js';
import { waitForEntityCount, getEntities, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const getConstraints = (page) => page.evaluate(() => window.__waffle.getConstraints());
const getCamera = (page) => page.evaluate(() => window.__waffle.getCameraState());

/** sketch-units-per-canvas-pixel, derived from a known drawn point. */
async function sketchUnitsPerPixel(page, pointId, canvasOffset) {
	const pos = await getPositions(page);
	return Math.abs(pos[pointId].x / canvasOffset);
}

/** Real mouse drag on the canvas from one offset to another, many events. */
async function realDrag(page, x1, y1, x2, y2, steps = 25) {
	const bounds = await getCanvasBounds(page);
	await page.mouse.move(bounds.centerX + x1, bounds.centerY + y1);
	await page.mouse.down();
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		// wiggle to fight the constraint manifold like a human hand
		const wx = 6 * Math.sin(i * 1.3);
		const wy = 8 * Math.sin(i * 0.7);
		await page.mouse.move(
			bounds.centerX + x1 + (x2 - x1) * t + wx,
			bounds.centerY + y1 + (y2 - y1) * t + wy
		);
		await page.waitForTimeout(25);
	}
	await page.mouse.up();
	await page.waitForTimeout(500);
}

async function buildUserFixture(page) {
	await clickSketch(page);
	await clickCenterRectangle(page);
	// inner rect: center at origin, corner at (75,75)px; outer at (100,100)px
	await drawRectangle(page, 0, 0, 75, 75);
	await waitForEntityCount(page, 11, 5000);
	await clickCenterRectangle(page);
	await drawRectangle(page, 0, 0, 100, 100);
	await waitForEntityCount(page, 21, 5000);

	const entities = await getEntities(page);
	const lines = entities.filter((e) => e.type === 'Line');
	const points = entities.filter((e) => e.type === 'Point');
	// Equal on the inner rect's bottom (l1: p1→p2) + right (l2: p2→p3) edges.
	await page.evaluate(
		([a, b]) => window.__waffle.addSketchConstraint({ type: 'Equal', entity_a: a, entity_b: b }),
		[lines[0].id, lines[1].id]
	);
	await page.waitForTimeout(400);
	return {
		// inner corners p1..p4; p2 is SHARED by the two Equal edges
		corners: [points[1], points[2], points[3], points[4]],
		centerId: points[0].id,
	};
}

test.describe('user repro: real-pointer corner drag, two nested center rects', () => {
	test.beforeEach(async ({ waffle }) => {
		// generous viewport-time per drag
		test.setTimeout(120000);
	});

	for (const cornerIdx of [0, 1, 2, 3]) {
		test(`drag inner corner p${cornerIdx + 1} — bounded, undo reverts drag not Equal`, async ({ waffle }) => {
			const page = waffle.page;
			const crashes = collectCrashErrors(page);
			const { corners, centerId } = await buildUserFixture(page);

			await clickSelect(page);
			const pos0 = await getPositions(page);
			const cons0 = await getConstraints(page);
			const nEqual0 = cons0.filter((c) => c.type === 'Equal').length;
			expect(nEqual0).toBe(1);
			const cam0 = await getCamera(page);

			const corner = corners[cornerIdx];
			const c0 = pos0[corner.id];
			// px-per-unit: inner corner p3 drawn at (75,75)px = |x| sketch units
			const upp = await sketchUnitsPerPixel(page, corners[2].id, 75);
			const cxPx = c0.x / upp;
			const cyPx = -c0.y / upp; // canvas y is flipped vs sketch y

			const extent0 = Math.max(
				...Object.values(pos0).flatMap((p) => [Math.abs(p.x), Math.abs(p.y)])
			);

			// Drag outward ~2.5x and around (screen coords; y flipped).
			await realDrag(page, cxPx, cyPx, cxPx * 2.5, cyPx * 2.5);

			const pos1 = await getPositions(page);
			for (const [id, p] of Object.entries(pos1)) {
				expect(Number.isFinite(p.x) && Number.isFinite(p.y), `point ${id} finite`).toBe(true);
				expect(
					Math.abs(p.x) < extent0 * 20 && Math.abs(p.y) < extent0 * 20,
					`point ${id} bounded, got (${p.x}, ${p.y})`
				).toBe(true);
			}
			// The center must still be locked at the origin after release.
			const center1 = pos1[centerId];
			expect(Math.hypot(center1.x, center1.y)).toBeLessThan(extent0 * 0.01);

			// Undo must revert the DRAG: Equal count unchanged, positions back.
			await page.keyboard.press('Control+z');
			await page.waitForTimeout(500);
			const cons2 = await getConstraints(page);
			expect(
				cons2.filter((c) => c.type === 'Equal').length,
				'undo must revert the drag, not the Equal constraint'
			).toBe(nEqual0);
			const pos2 = await getPositions(page);
			expect(pos2[corner.id].x).toBeCloseTo(c0.x, 4);
			expect(pos2[corner.id].y).toBeCloseTo(c0.y, 4);

			// And the camera must be back where it was before the drag.
			const cam2 = await getCamera(page);
			if (cam0.frustumTop != null && cam2.frustumTop != null) {
				expect(cam2.frustumTop).toBeCloseTo(cam0.frustumTop, 5);
			}
			for (let i = 0; i < 3; i++) {
				expect(cam2.position[i]).toBeCloseTo(cam0.position[i], 5);
			}

			expectNoAnyCrash(crashes);
		});
	}
});
