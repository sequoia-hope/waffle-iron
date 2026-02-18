/**
 * Auto-union failure detection tests (GUI).
 *
 * These tests detect when boss extrude auto-union silently falls back to a
 * standalone body. The engine swallows boolean union failures:
 *
 *   match execute_boolean(..., BooleanKind::Union) {
 *       Ok(result) => Ok(result),
 *       Err(_) => Ok(extrude_result),  // silent fallback
 *   }
 *
 * Detection signals:
 *   - Union SUCCESS: boss mesh has more triangles than base, bbox grows
 *   - Union FAILURE: boss mesh ≈ standalone box (few triangles, no bbox growth)
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	getMeshes,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Create a base box via the __waffle API.
 * Returns after mesh is verified.
 */
async function createBaseBox(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: -30, y: -30 });
		w.addSketchEntity({ type: 'Point', id: 2, x: 30, y: -30 });
		w.addSketchEntity({ type: 'Point', id: 3, x: 30, y: 30 });
		w.addSketchEntity({ type: 'Point', id: 4, x: -30, y: 30 });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	});
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1,
		{ timeout: 10000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(() => window.__waffle.applyExtrude(60, 0, false));

	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 2,
		{ timeout: 10000 }
	);
	await waitForMeshWithGeometry(page);
	await page.waitForTimeout(300);
}

/**
 * Create a boss box on the top face of an existing body.
 * Boss is a smaller rectangle sketched on the top face, extruded upward.
 * Uses merge=true (default) which triggers auto-union.
 */
async function createBossOnTop(page, expectedFeaturesBefore) {
	// Sketch on top face (z = 60 since base was extruded 60 units)
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 60], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 101, x: -15, y: -15 });
		w.addSketchEntity({ type: 'Point', id: 102, x: 15, y: -15 });
		w.addSketchEntity({ type: 'Point', id: 103, x: 15, y: 15 });
		w.addSketchEntity({ type: 'Point', id: 104, x: -15, y: 15 });
		w.addSketchEntity({ type: 'Line', id: 105, start_id: 101, end_id: 102, construction: false });
		w.addSketchEntity({ type: 'Line', id: 106, start_id: 102, end_id: 103, construction: false });
		w.addSketchEntity({ type: 'Line', id: 107, start_id: 103, end_id: 104, construction: false });
		w.addSketchEntity({ type: 'Line', id: 108, start_id: 104, end_id: 101, construction: false });
	});
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, expectedFeaturesBefore + 1, 10000);
	await page.waitForTimeout(200);

	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	// Extrude 30 units upward (merge=true, cut=false → auto-union)
	await page.evaluate(() => window.__waffle.applyExtrude(30, 0, false));

	await waitForFeatureCount(page, expectedFeaturesBefore + 2, 10000);
	await waitForMeshWithGeometry(page);
	await page.waitForTimeout(300);
}

test.describe('auto-union detection', () => {
	test.fixme('boss mesh has more triangles than base mesh alone', async ({ waffle }) => {
		// Step 1: Create base box and record its mesh stats
		await createBaseBox(waffle.page);
		let meshes = await getMeshes(waffle.page);
		const baseMeshTriangles = meshes.reduce((sum, m) => sum + m.triangleCount, 0);
		expect(baseMeshTriangles).toBeGreaterThan(0);

		// Step 2: Create boss on top face (auto-union should merge)
		await createBossOnTop(waffle.page, 2);
		meshes = await getMeshes(waffle.page);
		const bossMeshTriangles = meshes.reduce((sum, m) => sum + m.triangleCount, 0);

		// AUTO-UNION DETECTION:
		// If union succeeded, the boss extrude's mesh is the merged body (more triangles).
		// If union failed silently, the boss mesh is just the standalone boss box
		// (similar or fewer triangles than the base, and there are now two separate meshes).
		expect(bossMeshTriangles).toBeGreaterThan(
			baseMeshTriangles,
			`AUTO-UNION DETECTION: Boss mesh triangles (${bossMeshTriangles}) should exceed ` +
			`base mesh triangles (${baseMeshTriangles}). If equal or less, the auto-union ` +
			`likely fell back to a standalone body.`
		);
	});

	test.fixme('combined bounding box grows after boss extrude', async ({ waffle }) => {
		// Step 1: Create base box and record bounding box
		await createBaseBox(waffle.page);
		const baseBbox = await waffle.page.evaluate(
			() => window.__waffle.getMeshBoundingBox()
		);
		expect(baseBbox).toBeTruthy();
		const baseZSize = baseBbox.size[2];
		expect(baseZSize).toBeGreaterThan(0);

		// Step 2: Create boss on top
		await createBossOnTop(waffle.page, 2);
		const bossBbox = await waffle.page.evaluate(
			() => window.__waffle.getMeshBoundingBox()
		);
		expect(bossBbox).toBeTruthy();
		const bossZSize = bossBbox.size[2];

		// AUTO-UNION DETECTION:
		// If union succeeded, the bbox Z-extent should grow (base 60 + boss 30 = 90).
		// If union failed, the bbox may not grow (boss is rendered separately but
		// overall bbox is still just ~60, or two separate boxes at different Z levels).
		expect(bossZSize).toBeGreaterThan(
			baseZSize + 10,
			`AUTO-UNION DETECTION: After boss extrude, Z extent (${bossZSize.toFixed(1)}) ` +
			`should be significantly larger than base Z extent (${baseZSize.toFixed(1)}). ` +
			`Expected ~90 (60 base + 30 boss). If not, auto-union may have failed.`
		);
	});

	test.fixme('API-created boss: vertex count indicates merged body', async ({ waffle }) => {
		// Step 1: Create base box
		await createBaseBox(waffle.page);
		let meshes = await getMeshes(waffle.page);
		const baseVertexCount = meshes.reduce((sum, m) => sum + m.vertexCount, 0);

		// Step 2: Create boss
		await createBossOnTop(waffle.page, 2);
		meshes = await getMeshes(waffle.page);
		const bossVertexCount = meshes.reduce((sum, m) => sum + m.vertexCount, 0);

		// AUTO-UNION DETECTION:
		// Merged body has more vertices than a standalone box.
		// A simple 6-face box has ~24 vertices (4 per face, non-shared in truck tessellation).
		// The merged L-shaped body should have many more.
		expect(bossVertexCount).toBeGreaterThan(
			baseVertexCount,
			`AUTO-UNION DETECTION: Boss mesh vertex count (${bossVertexCount}) should ` +
			`exceed base vertex count (${baseVertexCount}). Equal counts suggest the ` +
			`auto-union produced two independent boxes instead of a merged body.`
		);

		// Also verify feature tree looks correct
		const tree = await waffle.page.evaluate(() => window.__waffle.getFeatureTree());
		expect(tree.features.length).toBe(4); // 2 sketches + 2 extrudes
	});
});
