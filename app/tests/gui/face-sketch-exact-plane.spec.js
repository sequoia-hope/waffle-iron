/**
 * A sketch drawn on a model face sits EXACTLY on that face's plane.
 *
 * The plane comes from the engine (every planar face range carries
 * `plane: { origin, normal }` in f64 — `wasm-bridge/src/render_view.rs`),
 * never from the render mesh, whose Float32 positions put a sketch origin
 * an f32 rounding off the face. That rounding is what made a through-cut's
 * caps miss a frame's faces by 2.235e-10 (`error_oct4.waffle`, 2026-10-04)
 * and sent the kernel through its near-coplanar weld (spec
 * `yang_455_coplanar_plane_weld.md` §7).
 *
 * Oracle: a 61.3 mm extrusion (61.3 is NOT representable in f32:
 * `Math.fround(61.3) !== 61.3`). Its +Z face range's origin z must be 61.3
 * to f64 rounding AND must not be an f32 value — a plane derived from the
 * render mesh is always exactly f32-representable. The sketch started on
 * that face takes the same origin bit for bit, and the finished sketch
 * feature stores it.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount, getMeshes, getFeatureTree } from './helpers/state.js';

const DEPTH_MM = 61.3;

/** The `createExtrudedBox` fixture at an f32-inexact depth. */
async function createBox(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);
	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: -30, y: -30, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: 30, y: -30, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: 30, y: 30, construction: false });
		w.addSketchEntity({ type: 'Point', id: 4, x: -30, y: 30, construction: false });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	});
	await page.waitForTimeout(200);
	await page.evaluate(() => void window.__waffle.finishSketch());
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1,
		{ timeout: 10000 }
	);
	await page.waitForTimeout(200);
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate((d) => void window.__waffle.applyExtrude(d, 0, false), DEPTH_MM);
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 2,
		{ timeout: 10000 }
	);
	await page.waitForFunction(
		() => (window.__waffle?.getMeshes() ?? []).some((m) => m.triangleCount > 0),
		{ timeout: 10000 }
	);
	await page.waitForTimeout(200);
}

/** The +Z cap's face range, with its engine plane. */
async function topFaceRange(page) {
	const meshes = await getMeshes(page);
	const ranges = meshes.flatMap((m) => m.faceRanges ?? []);
	expect(ranges.length).toBe(6);
	for (const r of ranges) {
		expect(r.plane, `face range without a plane: ${JSON.stringify(r.geom_ref)}`).toBeTruthy();
		const n = r.plane.normal;
		expect(Math.hypot(n[0], n[1], n[2])).toBeCloseTo(1, 12);
	}
	const top = ranges.find((r) => r.plane.normal[2] > 0.5);
	expect(top).toBeTruthy();
	return top;
}

/** `z` is the face's height to f64 rounding and is NOT an f32 value. */
function expectExactHeight(z) {
	expect(Math.fround(DEPTH_MM)).not.toBe(DEPTH_MM); // the oracle can tell f32 apart
	expect(Math.abs(z - DEPTH_MM)).toBeLessThan(1e-9);
	expect(Math.fround(z)).not.toBe(z);
}

test.describe('Sketch on face — exact plane', () => {
	test('every planar face range carries the engine plane; the +Z cap is not f32-quantized', async ({ waffle }) => {
		const page = waffle.page;
		await createBox(page);
		const top = await topFaceRange(page);
		expectExactHeight(top.plane.origin[2]);
	});

	test('a sketch started on the +Z face takes the exact plane and stores it', async ({ waffle }) => {
		const page = waffle.page;
		await createBox(page);
		const top = await topFaceRange(page);

		await page.evaluate((ref) => window.__waffle.selectRef(ref), top.geom_ref);
		await page.waitForTimeout(200);
		await clickSketch(page);
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		const mode = await page.evaluate(() => window.__waffle.getState().sketchMode);
		expect(mode.origin).toEqual(top.plane.origin);
		expectExactHeight(mode.origin[2]);

		await clickRectangle(page);
		await drawRectangle(page, -20, -15, 20, 15);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 3, 10000);

		const tree = await getFeatureTree(page);
		const sketches = tree.features.filter((f) => f.operation?.type === 'Sketch');
		const stored = sketches[sketches.length - 1].operation.sketch;
		// The feature tree reads back in document units (mm).
		expectExactHeight(stored.plane_origin[2]);
		expect(stored.plane_normal).toEqual([0, 0, 1]);
	});
});
