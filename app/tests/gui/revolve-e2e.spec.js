/**
 * Revolve end-to-end tests — verifies revolve creates actual 3D mesh geometry.
 *
 * The existing revolve.spec.js tests dialog lifecycle only.
 * This file tests the actual revolve-to-mesh pipeline at various angles.
 */
import { test, expect } from './helpers/waffle-test.js';
import { pickOffsetRevolveAxis } from './helpers/revolve.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickRevolve,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	getFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getMeshes,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle and finish it.
 */
async function createFinishedSketch(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('revolve-e2e-draw-failed');
	}
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('revolve-e2e-finish-failed');
	}
}

/**
 * Helper: open revolve dialog, set angle, and apply.
 */
async function applyRevolve(waffle, angle = '360') {
	await clickRevolve(waffle.page);
	await pickOffsetRevolveAxis(waffle.page);
	const angleInput = waffle.page.locator('#revolve-angle');
	await angleInput.fill(angle);
	await waffle.page.locator('[data-testid="revolve-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 15000); } catch {
		await waffle.dumpState(`revolve-e2e-apply-${angle}-failed`);
	}
}

test.describe('revolve end-to-end', () => {
	test('revolve 360 creates solid with mesh geometry', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await applyRevolve(waffle, '360');

		const featureCount = await getFeatureCount(waffle.page);
		expect(featureCount).toBe(2);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('revolve 180 creates half solid', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await applyRevolve(waffle, '180');

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		// Store triangle count for reference (half revolution should produce geometry)
		const meshes = await getMeshes(waffle.page);
		const meshWithGeo = meshes.find(m => m.triangleCount > 0);
		expect(meshWithGeo).toBeDefined();
		expect(meshWithGeo.triangleCount).toBeGreaterThan(0);
	});

	test('revolve 90 creates quarter solid', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await applyRevolve(waffle, '90');

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('revolve feature appears in tree as Revolve type', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await applyRevolve(waffle);

		const hasRevolve = await hasFeatureOfType(waffle.page, 'Revolve');
		expect(hasRevolve).toBe(true);

		const featureCount = await getFeatureCount(waffle.page);
		expect(featureCount).toBe(2);
	});
});

test.describe('revolve mesh verification', () => {
	test('full revolution (360) mesh has vertices and normals', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await applyRevolve(waffle, '360');

		const meshes = await getMeshes(waffle.page);
		const meshWithGeo = meshes.find(m => m.triangleCount > 0);
		expect(meshWithGeo).toBeDefined();
		expect(meshWithGeo.vertexCount).toBeGreaterThan(0);
		expect(meshWithGeo.hasNormals).toBe(true);
		expect(meshWithGeo.hasIndices).toBe(true);
	});

	test('partial revolution (45) produces mesh with geometry', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await applyRevolve(waffle, '45');

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		const meshes = await getMeshes(waffle.page);
		const meshWithGeo = meshes.find(m => m.triangleCount > 0);
		expect(meshWithGeo).toBeDefined();
		expect(meshWithGeo.triangleCount).toBeGreaterThan(0);
	});

	test('revolve mesh has bounding box', async ({ waffle }) => {
		await createFinishedSketch(waffle);
		await applyRevolve(waffle, '180');

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();
		// Bounding box should have non-zero size in all dimensions for a 3D solid
		expect(bbox.size[0]).toBeGreaterThan(0);
		expect(bbox.size[1]).toBeGreaterThan(0);
		expect(bbox.size[2]).toBeGreaterThan(0);
	});
});
