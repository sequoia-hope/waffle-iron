/**
 * Bearing recess — partial-depth circle cut into a cylinder cap (the user's
 * coplanar M8 scenario).
 *
 * Steps:
 *   1. Draw a circle → extrude to a cylinder (depth 10).
 *   2. Select the top cap face → draw a smaller circle on it.
 *   3. Extrude-CUT a PARTIAL depth (4 < 10) → a blind recess.
 *
 * The cut tool's near cap is coplanar with (and contained in) the cylinder
 * cap → Stage-0 disc∩disc coplanar containment; the cut floor stays inside the
 * body. This previously failed in-app with
 *   BooleanFailed("yang-rs: geometric face resolution failed for kept triangle
 *   14 (centroid off all face surfaces …)")
 * at real model scale (fixed: Stage-6 face attribution uses the Stage-0 weld
 * band, not absolute TAU_WORK). Unlike the full-depth cut, the partial cut
 * makes a coplanar pair at ONE cap only (a true recess, not a through-hole).
 *
 * This is the GUI canary for the coplanar bearing-recess + scale-tolerance fix:
 * the cut must succeed (no feature error / no crash) and leave a valid solid.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickCircle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawCircle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';
import { getFirstFaceRef } from './helpers/geometry.js';

async function selectFaceRef(page, ref) {
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(200);
}

// Feature rebuild errors (the bearing-recess failure surfaces here, NOT as a
// WASM crash). Returns an array of error strings.
async function getFeatureErrors(page) {
	return page.evaluate(() => {
		const errors = window.__waffle?.getFeatureErrors?.();
		if (!errors || !(errors instanceof Map)) return [];
		return Array.from(errors.values()).map((e) => String(e));
	});
}

// Build a cylinder (circle radius `r0` px → extrude `depth0`), then cut a
// concentric/off-center circle (radius `r1` px at offset `ox,oy`) `cutDepth`
// deep into the selected cap. Returns { errors, hasMesh, crashTracker }.
async function bearingRecess(page, { r0, depth0, ox, oy, r1, cutDepth }) {
	// 1. Cylinder.
	await clickSketch(page, 'front');
	await clickCircle(page);
	await drawCircle(page, 0, 0, r0, 0);
	await waitForEntityCount(page, 2, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);

	await clickExtrude(page);
	await page.locator('[data-testid="extrude-depth"]').fill(String(depth0));
	await page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(page, 2, 15000);
	await waitForMeshWithGeometry(page);

	// 2. Sketch a smaller circle on a cap face.
	const faceRef = await getFirstFaceRef(page);
	expect(faceRef).toBeTruthy();
	await selectFaceRef(page, faceRef);
	await clickSketch(page);
	await clickCircle(page);
	await drawCircle(page, ox, oy, ox + r1, oy);
	await waitForEntityCount(page, 2, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 3, 10000);

	// 3. PARTIAL-depth extrude-cut → blind recess.
	await clickExtrude(page);
	await page.locator('[data-testid="extrude-depth"]').fill(String(cutDepth));
	await page.locator('[data-testid="extrude-cut"]').check();
	await page.locator('[data-testid="extrude-apply"]').click();
	try {
		await waitForFeatureCount(page, 4, 30000);
	} catch {
		// non-fatal: the assertions below capture the real failure mode
	}

	const errors = await getFeatureErrors(page);
	const hasMesh = await hasMeshWithGeometry(page);
	return { errors, hasMesh };
}

test.describe('bearing recess (partial-depth coplanar circle cut)', () => {
	test('concentric recess cuts cleanly (no face-resolution / boolean error)', async ({
		waffle,
	}) => {
		const crashTracker = collectCrashErrors(waffle.page);
		const { errors, hasMesh } = await bearingRecess(waffle.page, {
			r0: 60,
			depth0: 10,
			ox: 0,
			oy: 0,
			r1: 30,
			cutDepth: 4,
		});

		const boolErrors = errors.filter((e) =>
			/face resolution|boolean .*failed|coplanar|no z overlap/i.test(e),
		);
		expect(boolErrors, `unexpected boolean errors: ${boolErrors.join(' | ')}`).toHaveLength(0);
		expect(errors, `unexpected feature errors: ${errors.join(' | ')}`).toHaveLength(0);
		expect(hasMesh, 'recessed solid must still have mesh geometry').toBe(true);
		expectNoAnyCrash(crashTracker);
	});
});
// (Off-center / scale-specific containment is covered at the Rust level in
// crates/test-harness/tests/bearing_recess_kv2.rs and the mm-scale fixture
// regression bearing_recess_mm_regression.rs.)
