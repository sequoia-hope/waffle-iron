/**
 * Offset datum plane tests — covers BOTH:
 *  - offset from a built-in PLANE (the pre-existing flow; must not regress), and
 *  - offset from a planar FACE (the new OffsetFromFace path), reached via the
 *    standalone "Plane" toolbar entry (datum-plane creation WITHOUT starting a
 *    sketch).
 *
 * Face selection is programmatic (`selectRef`) because real 3D face picking
 * needs pixel-perfect camera raycasting in headless; the datum-plane creation
 * itself goes through real toolbar + dialog button clicks. Per project GUI
 * rules: no try/catch around expected-state waits, and crashes are asserted
 * with collectCrashErrors + expectNoAnyCrash.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

const FRONT_PLANE_ID = '00000000-0000-0000-0000-000000000001';

/** Create a sketch + extruded box via real GUI events. Leaves 2 features. */
async function createExtrudedBox(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	await waitForEntityCount(waffle.page, 8, 5000);

	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);

	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);
}

/** Get a planar face GeomRef from the first mesh that has face ranges. */
async function getFirstFaceRef(page) {
	return page.evaluate(() => {
		const meshes = window.__waffle.getMeshes();
		const mesh = meshes.find((m) => m.faceRangeCount > 0);
		if (!mesh || mesh.faceRanges.length === 0) return null;
		return mesh.faceRanges[0].geom_ref;
	});
}

function dot(a, b) {
	return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

test.describe('offset datum plane creation', () => {
	test('standalone toolbar entry creates an offset plane FROM a selected face', async ({
		waffle,
	}) => {
		const crashes = collectCrashErrors(waffle.page);

		await createExtrudedBox(waffle);

		// Resolve the base face plane (the "must match" reference) up front.
		const faceRef = await getFirstFaceRef(waffle.page);
		expect(faceRef).toBeTruthy();
		const basePlane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			faceRef
		);
		expect(basePlane).not.toBeNull();

		// Select the face, then open the STANDALONE datum-plane flow (no sketch).
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), faceRef);
		await waffle.page.waitForTimeout(150);

		await waffle.page.locator('[data-testid="toolbar-btn-datum-plane"]').click();
		await waffle.page
			.locator('[data-testid="sketch-plane-dialog"]')
			.waitFor({ state: 'visible', timeout: 5000 });

		// With a face selected, the dialog should default to the face base.
		await waffle.page
			.locator('[data-testid="offset-base-face"]')
			.waitFor({ state: 'visible', timeout: 5000 });

		// Distance is entered in the document display unit (mm) and converted to
		// internal meters by the dialog (like ExtrudeDialog).
		const distanceMm = 25;
		const distanceInternal = distanceMm / 1000;
		await waffle.page.locator('[data-testid="offset-distance-input"]').fill(String(distanceMm));
		await waffle.page.locator('[data-testid="offset-create-btn"]').click();

		// A new DatumPlane feature must appear in the tree (3rd feature).
		await waitForFeatureCount(waffle.page, 3, 10000);

		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		const datum = tree.features.find((f) => f.operation?.type === 'DatumPlane');
		expect(datum).toBeTruthy();
		expect(datum.operation.params.definition.method).toBe('offset-face');

		// The datum must resolve to base-face-plane + distance*normal, using the
		// LIVE face resolver (same path DatumVis renders with). Distance is in
		// the app's working unit (mm in the UI → meters internally is handled by
		// the engine; here resolvePlaneById uses the raw stored definition).
		const resolved = await waffle.page.evaluate(
			(id) => window.__waffle.resolvePlaneById(id),
			datum.id
		);
		expect(resolved).not.toBeNull();

		// Normal matches the base face normal exactly.
		for (let k = 0; k < 3; k++) {
			expect(resolved.normal[k]).toBeCloseTo(basePlane.normal[k], 9);
		}
		// Origin sits `distance` off the face along its normal.
		const signed = dot(
			[
				resolved.origin[0] - basePlane.origin[0],
				resolved.origin[1] - basePlane.origin[1],
				resolved.origin[2] - basePlane.origin[2],
			],
			basePlane.normal
		);
		expect(signed).toBeCloseTo(distanceInternal, 6);

		// Starting a sketch on the datum lands on the SAME resolved plane —
		// proves the datum is consumable (not frozen / not divergent).
		const datumRef = {
			kind: { type: 'Face' },
			anchor: { type: 'DatumPlane', id: datum.id },
		};
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), datumRef);
		await waffle.page.waitForTimeout(150);
		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		for (let k = 0; k < 3; k++) {
			expect(state.sketchMode.normal[k]).toBeCloseTo(resolved.normal[k], 9);
			expect(state.sketchMode.origin[k]).toBeCloseTo(resolved.origin[k], 6);
		}

		expectNoAnyCrash(crashes);
	});

	test('offset from a built-in PLANE still works (no regression)', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		// Open the standalone datum-plane flow with nothing selected → plane base.
		await waffle.page.locator('[data-testid="toolbar-btn-datum-plane"]').click();
		await waffle.page
			.locator('[data-testid="sketch-plane-dialog"]')
			.waitFor({ state: 'visible', timeout: 5000 });

		// Plane dropdown is the base (no face selected).
		await waffle.page
			.locator('[data-testid="offset-base-select"]')
			.waitFor({ state: 'visible', timeout: 5000 });
		await waffle.page.locator('[data-testid="offset-base-select"]').selectOption(FRONT_PLANE_ID);

		const distanceMm = 15;
		const distanceInternal = distanceMm / 1000;
		await waffle.page.locator('[data-testid="offset-distance-input"]').fill(String(distanceMm));
		await waffle.page.locator('[data-testid="offset-create-btn"]').click();

		await waitForFeatureCount(waffle.page, 1, 10000);

		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		const datum = tree.features.find((f) => f.operation?.type === 'DatumPlane');
		expect(datum).toBeTruthy();
		expect(datum.operation.params.definition.method).toBe('offset');

		const resolved = await waffle.page.evaluate(
			(id) => window.__waffle.resolvePlaneById(id),
			datum.id
		);
		expect(resolved).not.toBeNull();
		// Front plane: origin [0,0,0], normal [0,0,1] → offset along +Z.
		expect(resolved.normal).toEqual([0, 0, 1]);
		expect(resolved.origin[2]).toBeCloseTo(distanceInternal, 6);

		expectNoAnyCrash(crashes);
	});

	// End-to-end alignment guard: draw an OFF-CENTRE rectangle on an offset plane,
	// extrude, and assert the resulting BODY lands exactly where the sketch is —
	// the near cap on the plane, the in-plane centre matching the sketch's world
	// centre. The prior offset-plane tests stopped at plane resolution / sketch
	// entry and never verified the extruded body's world position (the user's
	// reported symptom). An off-centre rect also catches an in-plane shift that a
	// symmetric rect would hide.
	test('off-centre rectangle on an offset plane extrudes a body that lines up', async ({
		waffle,
	}) => {
		const crashes = collectCrashErrors(waffle.page);

		// Offset Front plane at 30mm (= 0.03m internal).
		await waffle.page.locator('[data-testid="toolbar-btn-datum-plane"]').click();
		await waffle.page
			.locator('[data-testid="sketch-plane-dialog"]')
			.waitFor({ state: 'visible', timeout: 5000 });
		await waffle.page.locator('[data-testid="offset-base-select"]').selectOption(FRONT_PLANE_ID);
		await waffle.page.locator('[data-testid="offset-distance-input"]').fill('30');
		await waffle.page.locator('[data-testid="offset-create-btn"]').click();
		await waitForFeatureCount(waffle.page, 1, 10000);

		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		const datum = tree.features.find((f) => f.operation?.type === 'DatumPlane');
		const datumRef = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: datum.id } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), datumRef);
		await waffle.page.waitForTimeout(150);
		await clickSketch(waffle.page);

		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, 40, 30, 160, 120); // off-centre
		await waitForEntityCount(waffle.page, 8, 5000);
		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 2, 10000);

		// Sketch's world centre = plane_origin + mean(u,v) in the buildSketchPlane basis.
		const sketchCenter = await waffle.page.evaluate(() => {
			const t = window.__waffle.getFeatureTree();
			const sk = t.features.find((f) => f.operation?.type === 'Sketch');
			const s = sk.operation.sketch;
			const o = s.plane_origin,
				n = s.plane_normal;
			const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
			const cross = (a, b) => [
				a[1] * b[2] - a[2] * b[1],
				a[2] * b[0] - a[0] * b[2],
				a[0] * b[1] - a[1] * b[0],
			];
			const ref = Math.abs(dot(n, [0, 0, 1])) < 0.99 ? [0, 0, 1] : [1, 0, 0];
			let x = cross(ref, n);
			let xl = Math.hypot(...x);
			x = x.map((v) => v / xl);
			let y = cross(n, x);
			let yl = Math.hypot(...y);
			y = y.map((v) => v / yl);
			const pos = Object.values(s.solved_positions || {});
			let cu = 0,
				cv = 0;
			for (const p of pos) {
				cu += p[0] ?? p.x;
				cv += p[1] ?? p.y;
			}
			cu /= pos.length;
			cv /= pos.length;
			return [o[0] + x[0] * cu + y[0] * cv, o[1] + x[1] * cu + y[1] * cv, o[2] + x[2] * cu + y[2] * cv];
		});

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(waffle.page, 3, 10000);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());

		// In-plane (x,y) body centre must coincide with the sketch's world centre.
		expect(Math.abs(bbox.center[0] - sketchCenter[0])).toBeLessThan(1e-4);
		expect(Math.abs(bbox.center[1] - sketchCenter[1])).toBeLessThan(1e-4);
		// Near cap on the plane (z = 0.03), not at the origin.
		expect(Math.min(bbox.min[2], bbox.max[2])).toBeCloseTo(0.03, 4);

		expectNoAnyCrash(crashes);
	});
});
