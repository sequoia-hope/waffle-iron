/**
 * Auto-union known edge cases — documenting known limitations.
 *
 * Auto-union happens when a boss extrude overlaps an existing body:
 * the engine attempts boolean union automatically. These tests document
 * known failure scenarios using test.fixme().
 *
 * See also: auto-union-detection.spec.js for triangle/bbox/vertex count
 * detection of silent auto-union fallback.
 *
 * Known failure modes:
 * - Coplanar faces: boss shares a face with the base body
 * - Edge-coincident: boss edge lies exactly on base body edge
 * - Tangential: cylindrical boss tangent to base body face
 * - Chained: multiple sequential auto-unions accumulate error
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	getFeatureCount,
	getMeshes,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Create a base box centered at origin, extruded along Z.
 */
async function createBaseBox(page, size = 30, depth = 60) {
	await page.evaluate(({ s }) => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]), { s: size });
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(({ s }) => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: -s, y: -s, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: s, y: -s, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: s, y: s, construction: false });
		w.addSketchEntity({ type: 'Point', id: 4, x: -s, y: s, construction: false });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	}, { s: size });
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1,
		{ timeout: 10000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(({ d }) => window.__waffle.applyExtrude(d, 0, false), { d: depth });

	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 2,
		{ timeout: 10000 }
	);
	await waitForMeshWithGeometry(page);
	await page.waitForTimeout(300);
}

/**
 * Create a boss sketch + extrude at the given origin/normal.
 */
async function createBoss(page, origin, normal, size, depth, expectedFeaturesBefore) {
	await page.evaluate(({ o, n }) => window.__waffle.enterSketch(o, n), { o: origin, n: normal });
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(({ s }) => {
		const w = window.__waffle;
		const base = 200;
		w.addSketchEntity({ type: 'Point', id: base + 1, x: -s, y: -s, construction: false });
		w.addSketchEntity({ type: 'Point', id: base + 2, x: s, y: -s, construction: false });
		w.addSketchEntity({ type: 'Point', id: base + 3, x: s, y: s, construction: false });
		w.addSketchEntity({ type: 'Point', id: base + 4, x: -s, y: s, construction: false });
		w.addSketchEntity({ type: 'Line', id: base + 5, start_id: base + 1, end_id: base + 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: base + 6, start_id: base + 2, end_id: base + 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: base + 7, start_id: base + 3, end_id: base + 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: base + 8, start_id: base + 4, end_id: base + 1, construction: false });
	}, { s: size });
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, expectedFeaturesBefore + 1, 10000);
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(({ d }) => window.__waffle.applyExtrude(d, 0, false), { d: depth });

	await waitForFeatureCount(page, expectedFeaturesBefore + 2, 10000);
	await waitForMeshWithGeometry(page);
	await page.waitForTimeout(300);
}

test.describe('auto-union edge cases', () => {
	test.fixme('coplanar face: boss flush with base top face', async ({ waffle }) => {
		// Boss shares the entire top face with base — coplanar face degenerate case.
		// The boss rectangle is the same size as the base, creating a flush extension.
		// Auto-union must handle the fully coplanar shared face.
		await createBaseBox(waffle.page, 30, 60);

		const baseMeshes = await getMeshes(waffle.page);
		const baseTriangles = baseMeshes.reduce((s, m) => s + m.triangleCount, 0);

		// Boss on top face, same size as base — fully coplanar shared face
		await createBoss(waffle.page, [0, 0, 60], [0, 0, 1], 30, 30, 2);

		const afterMeshes = await getMeshes(waffle.page);
		const afterTriangles = afterMeshes.reduce((s, m) => s + m.triangleCount, 0);

		// If union succeeded, the merged body should have more triangles
		// than the base alone (it's now a taller box).
		expect(afterTriangles).toBeGreaterThan(
			baseTriangles,
			`Coplanar auto-union: expected merged body to have more triangles ` +
			`(${afterTriangles}) than base (${baseTriangles}). ` +
			`Flush coplanar face may cause boolean failure.`
		);
	});

	test.fixme('edge-coincident: boss edge aligns with base edge', async ({ waffle }) => {
		// Boss is placed so its edge lies exactly on the base body's edge.
		// This creates an edge-coincident degenerate case.
		await createBaseBox(waffle.page, 30, 60);

		const baseMeshes = await getMeshes(waffle.page);
		const baseTriangles = baseMeshes.reduce((s, m) => s + m.triangleCount, 0);

		// Boss on top face, shifted so left edge aligns with base left edge
		// Base goes from -30 to +30. Boss from -30 to 0 (shares left edge).
		await waffle.page.evaluate(() => window.__waffle.enterSketch([0, 0, 60], [0, 0, 1]));
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 301, x: -30, y: -15, construction: false });
			w.addSketchEntity({ type: 'Point', id: 302, x: 0, y: -15, construction: false });
			w.addSketchEntity({ type: 'Point', id: 303, x: 0, y: 15, construction: false });
			w.addSketchEntity({ type: 'Point', id: 304, x: -30, y: 15, construction: false });
			w.addSketchEntity({ type: 'Line', id: 305, start_id: 301, end_id: 302, construction: false });
			w.addSketchEntity({ type: 'Line', id: 306, start_id: 302, end_id: 303, construction: false });
			w.addSketchEntity({ type: 'Line', id: 307, start_id: 303, end_id: 304, construction: false });
			w.addSketchEntity({ type: 'Line', id: 308, start_id: 304, end_id: 301, construction: false });
		});
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => window.__waffle.finishSketch());
		await waitForFeatureCount(waffle.page, 3, 10000);
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
		await waffle.page.waitForTimeout(100);
		await waffle.page.evaluate(() => window.__waffle.applyExtrude(20, 0, false));
		await waitForFeatureCount(waffle.page, 4, 10000);
		await waitForMeshWithGeometry(waffle.page);
		await waffle.page.waitForTimeout(300);

		const afterMeshes = await getMeshes(waffle.page);
		const afterTriangles = afterMeshes.reduce((s, m) => s + m.triangleCount, 0);

		expect(afterTriangles).toBeGreaterThan(
			baseTriangles,
			`Edge-coincident auto-union: boss shares an edge with base body. ` +
			`Expected merged body triangles (${afterTriangles}) > base (${baseTriangles}).`
		);
	});

	test.fixme('chained auto-unions: 3 sequential bosses', async ({ waffle }) => {
		// Multiple sequential auto-unions accumulate numerical error.
		// Each boolean result feeds into the next, and imprecision cascades.
		await createBaseBox(waffle.page, 30, 60);

		const baseMeshes = await getMeshes(waffle.page);
		const baseTriangles = baseMeshes.reduce((s, m) => s + m.triangleCount, 0);

		// Boss 1: small box on top
		await createBoss(waffle.page, [0, 0, 60], [0, 0, 1], 10, 15, 2);

		const afterBoss1 = await getMeshes(waffle.page);
		const boss1Triangles = afterBoss1.reduce((s, m) => s + m.triangleCount, 0);

		// Boss 2: another small box on the side
		await waffle.page.evaluate(() => window.__waffle.enterSketch([30, 0, 30], [1, 0, 0]));
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 401, x: -10, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 402, x: 10, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 403, x: 10, y: 10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 404, x: -10, y: 10, construction: false });
			w.addSketchEntity({ type: 'Line', id: 405, start_id: 401, end_id: 402, construction: false });
			w.addSketchEntity({ type: 'Line', id: 406, start_id: 402, end_id: 403, construction: false });
			w.addSketchEntity({ type: 'Line', id: 407, start_id: 403, end_id: 404, construction: false });
			w.addSketchEntity({ type: 'Line', id: 408, start_id: 404, end_id: 401, construction: false });
		});
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => window.__waffle.finishSketch());
		await waitForFeatureCount(waffle.page, 5, 10000);
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
		await waffle.page.waitForTimeout(100);
		await waffle.page.evaluate(() => window.__waffle.applyExtrude(15, 0, false));
		await waitForFeatureCount(waffle.page, 6, 10000);
		await waitForMeshWithGeometry(waffle.page);
		await waffle.page.waitForTimeout(300);

		// Boss 3: yet another box on the other side
		await waffle.page.evaluate(() => window.__waffle.enterSketch([-30, 0, 30], [-1, 0, 0]));
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 501, x: -10, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 502, x: 10, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 503, x: 10, y: 10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 504, x: -10, y: 10, construction: false });
			w.addSketchEntity({ type: 'Line', id: 505, start_id: 501, end_id: 502, construction: false });
			w.addSketchEntity({ type: 'Line', id: 506, start_id: 502, end_id: 503, construction: false });
			w.addSketchEntity({ type: 'Line', id: 507, start_id: 503, end_id: 504, construction: false });
			w.addSketchEntity({ type: 'Line', id: 508, start_id: 504, end_id: 501, construction: false });
		});
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => window.__waffle.finishSketch());
		await waitForFeatureCount(waffle.page, 7, 10000);
		await waffle.page.waitForTimeout(200);

		await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
		await waffle.page.waitForTimeout(100);
		await waffle.page.evaluate(() => window.__waffle.applyExtrude(15, 0, false));
		await waitForFeatureCount(waffle.page, 8, 10000);
		await waitForMeshWithGeometry(waffle.page);
		await waffle.page.waitForTimeout(300);

		const finalMeshes = await getMeshes(waffle.page);
		const finalTriangles = finalMeshes.reduce((s, m) => s + m.triangleCount, 0);

		// After 3 chained unions, the body should have significantly more
		// triangles than the original base. If any union silently failed,
		// we'd see separate bodies instead of one merged body.
		expect(finalTriangles).toBeGreaterThan(
			boss1Triangles,
			`Chained auto-union: after 3 sequential bosses, expected final body ` +
			`triangles (${finalTriangles}) > after first boss (${boss1Triangles}). ` +
			`Chained boolean cascades may accumulate error.`
		);
	});

	test.fixme('small boss near base corner triggers degenerate geometry', async ({ waffle }) => {
		// A very small boss placed near a corner of the base body creates
		// near-degenerate intersection curves that can cause boolean failure.
		await createBaseBox(waffle.page, 30, 60);

		const baseMeshes = await getMeshes(waffle.page);
		const baseTriangles = baseMeshes.reduce((s, m) => s + m.triangleCount, 0);

		// Tiny boss near the corner of the top face
		await createBoss(waffle.page, [25, 25, 60], [0, 0, 1], 5, 10, 2);

		const afterMeshes = await getMeshes(waffle.page);
		const afterTriangles = afterMeshes.reduce((s, m) => s + m.triangleCount, 0);

		expect(afterTriangles).toBeGreaterThan(
			baseTriangles,
			`Corner boss auto-union: tiny boss near base corner. ` +
			`Expected merged body (${afterTriangles}) > base (${baseTriangles}). ` +
			`Near-corner geometry creates degenerate intersection curves.`
		);
	});
});
