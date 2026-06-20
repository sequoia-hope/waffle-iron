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

		const distance = 25;
		await waffle.page.locator('[data-testid="offset-distance-input"]').fill(String(distance));
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
		expect(signed).toBeCloseTo(distance, 6);

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

		const distance = 15;
		await waffle.page.locator('[data-testid="offset-distance-input"]').fill(String(distance));
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
		expect(resolved.origin[2]).toBeCloseTo(distance, 6);

		expectNoAnyCrash(crashes);
	});
});
