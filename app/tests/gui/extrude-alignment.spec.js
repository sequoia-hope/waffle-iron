/**
 * Extrude alignment tests — verifies tangent_x_from_normal() in Rust
 * produces the same coordinate frame as buildSketchPlane() in JS.
 *
 * Bug: Old tangent_x_from_normal used a different formula that mapped sketch
 * coordinates to wrong world-space axes. For XY plane (normal=[0,0,1]):
 *   Old: xAxis = [0,+1,0]  (sketch X -> world +Y)
 *   New: xAxis = [0,-1,0]  (sketch X -> world -Y)
 *
 * These tests use the getMeshBoundingBox API to assert bounding box coordinates
 * after extrude. The key discriminating assertion is the sign of the Y coordinate:
 * old code produces positive Y, new code produces negative Y.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle using API entities for precise coordinates.
 * Uses addSketchEntity to place points at exact sketch-space positions.
 */
async function createPreciseRectangleSketch(page, { x1, y1, x2, y2 }) {
	// Enter sketch mode on XY plane (normal=[0,0,1])
	await page.evaluate(async () => {
		await window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
		window.__waffle.setTool('select');
	});
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	// Add rectangle entities at precise sketch coordinates via API
	await page.evaluate(({ x1, y1, x2, y2 }) => {
		const w = window.__waffle;
		// Four corners as points
		w.addSketchEntity({ id: 1, type: 'Point', x: x1, y: y1 });
		w.addSketchEntity({ id: 2, type: 'Point', x: x2, y: y1 });
		w.addSketchEntity({ id: 3, type: 'Point', x: x2, y: y2 });
		w.addSketchEntity({ id: 4, type: 'Point', x: x1, y: y2 });
		// Four lines forming closed rectangle
		w.addSketchEntity({ id: 5, type: 'Line', start_id: 1, end_id: 2 });
		w.addSketchEntity({ id: 6, type: 'Line', start_id: 2, end_id: 3 });
		w.addSketchEntity({ id: 7, type: 'Line', start_id: 3, end_id: 4 });
		w.addSketchEntity({ id: 8, type: 'Line', start_id: 4, end_id: 1 });
	}, { x1, y1, x2, y2 });

	await waitForEntityCount(page, 8, 3000);

	// Finish sketch
	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, 1, 10000);
}

test.describe('extrude alignment with sketch coordinate frame', () => {
	test('XY plane extrude maps sketch X to world -Y (not +Y)', async ({ waffle }) => {
		// Create a rectangle at sketch coords (5,5) -> (15,15) on XY plane
		await createPreciseRectangleSketch(waffle.page, { x1: 5, y1: 5, x2: 15, y2: 15 });

		// Show extrude dialog and apply depth=10
		await waffle.page.evaluate(async () => {
			window.__waffle.showExtrudeDialog();
		});
		await waffle.page.waitForFunction(
			() => window.__waffle?.getExtrudeDialogState() !== null,
			{ timeout: 5000 }
		);
		await waffle.page.evaluate(async () => {
			await window.__waffle.applyExtrude(10, 0, false);
		});

		// Wait for mesh generation
		await waitForMeshWithGeometry(waffle.page, 15000);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();

		// For XY plane (normal=[0,0,1]):
		//   xAxis = [0,-1,0] (ref=[1,0,0] x n=[0,0,1])
		//   yAxis = n x xAxis = [0,0,1] x [0,-1,0] = [1,0,0]
		//
		// Sketch coords (5,15) x (5,15) map to world:
		//   world = origin + sketchX * xAxis + sketchY * yAxis
		//   worldX component: sketchY * yAxis.x = sketchY * 1 => range [5, 15]
		//   worldY component: sketchX * xAxis.y = sketchX * (-1) => range [-15, -5]
		//   worldZ component: extrude along normal [0,0,1] => range [0, 10]

		// KEY DISCRIMINATING ASSERTION:
		// Old code had xAxis = [0,+1,0], which would put Y in range [+5, +15]
		// New code has xAxis = [0,-1,0], which puts Y in range [-15, -5]
		expect(bbox.max[1]).toBeLessThan(0);  // Would be > 0 with old buggy code!

		// Full coordinate checks
		expect(bbox.min[0]).toBeCloseTo(5, 0);
		expect(bbox.max[0]).toBeCloseTo(15, 0);
		expect(bbox.min[1]).toBeCloseTo(-15, 0);
		expect(bbox.max[1]).toBeCloseTo(-5, 0);
		expect(bbox.min[2]).toBeCloseTo(0, 0);
		expect(bbox.max[2]).toBeCloseTo(10, 0);

		// Extrude depth along Z
		expect(bbox.size[2]).toBeCloseTo(10, 0);
	});

	test('XY plane bbox size matches sketch rectangle dimensions', async ({ waffle }) => {
		await createPreciseRectangleSketch(waffle.page, { x1: 0, y1: 0, x2: 20, y2: 30 });

		await waffle.page.evaluate(async () => {
			window.__waffle.showExtrudeDialog();
		});
		await waffle.page.waitForFunction(
			() => window.__waffle?.getExtrudeDialogState() !== null,
			{ timeout: 5000 }
		);
		await waffle.page.evaluate(async () => {
			await window.__waffle.applyExtrude(5, 0, false);
		});

		await waitForMeshWithGeometry(waffle.page, 15000);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();

		// Sketch (0,0)-(20,30) on XY plane:
		//   worldX = sketchY => range [0, 30]
		//   worldY = -sketchX => range [-20, 0]
		//   worldZ = extrude depth => range [0, 5]

		// Size in world X should be 30 (from sketch Y dimension)
		expect(bbox.size[0]).toBeCloseTo(30, 0);
		// Size in world Y should be 20 (from sketch X dimension)
		expect(bbox.size[1]).toBeCloseTo(20, 0);
		// Size in world Z should be 5 (extrude depth)
		expect(bbox.size[2]).toBeCloseTo(5, 0);

		// Y coordinates must be non-positive (xAxis=[0,-1,0])
		expect(bbox.max[1]).toBeLessThanOrEqual(0.01);
	});

	test('small off-origin square on XY plane extrudes lined up (3mm, offset)', async ({ waffle }) => {
		// The user's reported scenario: a SMALL (3×3mm) square on a primary plane,
		// positioned in-plane AWAY from the origin (near corner 5mm out), at real
		// model scale (metres). It must extrude exactly where the sketch is — not
		// collapse toward the sketch origin and not malform at small scale.
		await createPreciseRectangleSketch(waffle.page, {
			x1: 0.005, y1: 0.005, x2: 0.008, y2: 0.008,
		});

		await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
		await waffle.page.waitForFunction(
			() => window.__waffle?.getExtrudeDialogState() !== null,
			{ timeout: 5000 }
		);
		await waffle.page.evaluate(() => window.__waffle.applyExtrude(0.002, 0, false));
		await waitForMeshWithGeometry(waffle.page, 15000);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();

		// XY basis: worldX = sketchY ∈ [0.005, 0.008]; worldY = -sketchX ∈ [-0.008, -0.005];
		// worldZ = depth ∈ [0, 0.002]. The body is a 3×3×2mm box sitting OFF the origin.
		expect(bbox.size[0]).toBeCloseTo(0.003, 4);
		expect(bbox.size[1]).toBeCloseTo(0.003, 4);
		expect(bbox.size[2]).toBeCloseTo(0.002, 4);
		// In-plane position matches the offset square (NOT centred on the origin).
		expect(bbox.min[0]).toBeCloseTo(0.005, 4);
		expect(bbox.max[0]).toBeCloseTo(0.008, 4);
		expect(bbox.min[1]).toBeCloseTo(-0.008, 4);
		expect(bbox.max[1]).toBeCloseTo(-0.005, 4);
	});

	test('GUI-drawn rectangle on XY produces extrude with negative Y coords', async ({ waffle }) => {
		// This test uses real GUI drawing (not API) to verify the full pipeline
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);

		// Draw a large rectangle offset to the right side of canvas
		await drawRectangle(waffle.page, 20, 20, 120, 80);

		try {
			await waitForEntityCount(waffle.page, 8, 5000);
		} catch {
			await waffle.dumpState('alignment-gui-draw-failed');
			throw new Error('Failed to draw rectangle for alignment test');
		}

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		// Extrude via API
		await waffle.page.evaluate(async () => {
			window.__waffle.showExtrudeDialog();
		});
		await waffle.page.waitForFunction(
			() => window.__waffle?.getExtrudeDialogState() !== null,
			{ timeout: 5000 }
		);
		await waffle.page.evaluate(async () => {
			await window.__waffle.applyExtrude(10, 0, false);
		});

		await waitForMeshWithGeometry(waffle.page, 15000);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();

		// For XY plane: xAxis=[0,-1,0], so sketch positive-X maps to world negative-Y.
		// A rectangle drawn with positive sketch-X coordinates must have some negative world-Y.
		// With old buggy code (xAxis=[0,+1,0]), all Y would be positive.
		// This is a weaker check that works regardless of exact pixel-to-sketch mapping.
		const hasNegativeY = bbox.min[1] < 0;
		expect(hasNegativeY).toBe(true);

		// Extrude should add Z extent
		expect(bbox.size[2]).toBeGreaterThan(0);
	});
});
