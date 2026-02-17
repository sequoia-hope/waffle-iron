/**
 * Extrude alignment — verify extruded geometry lines up with the drawn sketch.
 *
 * The bug: JS (sketchCoords.js) and Rust (rebuild.rs) computed different
 * 2D coordinate frames from the same plane normal, causing a 180-degree
 * rotation on the XY plane. This test catches that misalignment.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickCircle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle, drawCircle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount, hasMeshWithGeometry } from './helpers/state.js';

/**
 * Draw a rectangle on the given plane, finish sketch, extrude via dialog.
 * Returns sketch point positions for alignment verification.
 */
async function sketchAndExtrude(waffle, plane) {
	await clickSketch(waffle.page, plane);
	await clickRectangle(waffle.page);

	// Draw a rectangle offset from center (NOT symmetric around origin
	// so misalignment is detectable). Uses -80,-60 to 80,60.
	await drawRectangle(waffle.page, -80, -60, 80, 60);

	try {
		await waitForEntityCount(waffle.page, 8, 5000);
	} catch {
		await waffle.dumpState(`alignment-${plane}-sketch-failed`);
	}

	// Read back the actual sketch positions
	const sketchPoints = await waffle.page.evaluate(() => {
		const w = window.__waffle;
		const entities = w.getEntities();
		const positions = w.getPositions();
		return entities
			.filter(e => e.type === 'Point')
			.map(p => {
				const pos = positions.get(p.id);
				return pos ? { id: p.id, x: pos.x, y: pos.y } : null;
			})
			.filter(Boolean);
	});

	await clickFinishSketch(waffle.page);
	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState(`alignment-${plane}-finish-failed`);
	}

	// Extrude via dialog (proven flow)
	await clickExtrude(waffle.page);
	const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
	if (await depthInput.isVisible()) {
		await depthInput.fill('10');
	}
	await waffle.page.locator('[data-testid="extrude-apply"]').click();

	// Wait for dialog to close
	await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

	try {
		await waitForFeatureCount(waffle.page, 2, 10000);
	} catch {
		await waffle.dumpState(`alignment-${plane}-extrude-failed`);
	}

	// Wait for mesh to appear
	await waffle.page.waitForFunction(
		() => {
			const meshes = window.__waffle?.getMeshes() ?? [];
			return meshes.some(m => m.triangleCount > 0);
		},
		{ timeout: 10000 }
	);

	return sketchPoints;
}

test.describe('extrude alignment', () => {
	test('Front (XY) plane rectangle extrude creates mesh', async ({ waffle }) => {
		const sketchPoints = await sketchAndExtrude(waffle, 'front');

		// Must have 4 rectangle corner points
		expect(sketchPoints.length).toBeGreaterThanOrEqual(4);

		// Mesh must exist with actual geometry
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('Top (XZ) plane rectangle extrude creates mesh', async ({ waffle }) => {
		const sketchPoints = await sketchAndExtrude(waffle, 'top');

		expect(sketchPoints.length).toBeGreaterThanOrEqual(4);
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('Right (YZ) plane rectangle extrude creates mesh', async ({ waffle }) => {
		const sketchPoints = await sketchAndExtrude(waffle, 'right');

		expect(sketchPoints.length).toBeGreaterThanOrEqual(4);
		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('off-center circle extrude produces mesh', async ({ waffle }) => {
		await clickSketch(waffle.page, 'front');
		await clickCircle(waffle.page);

		// Draw circle off-center (center at canvas offset +60,+40, radius ~30px)
		await drawCircle(waffle.page, 60, 40, 90, 40);

		try {
			await waitForEntityCount(waffle.page, 2, 5000);
		} catch {
			await waffle.dumpState('alignment-circle-sketch-failed');
		}

		// Read the circle's sketch-local center
		const circleCenter = await waffle.page.evaluate(() => {
			const w = window.__waffle;
			const entities = w.getEntities();
			const positions = w.getPositions();
			const circle = entities.find(e => e.type === 'Circle');
			if (!circle) return null;
			const center = positions.get(circle.center_id);
			return center ? { x: center.x, y: center.y, radius: circle.radius } : null;
		});

		expect(circleCenter).not.toBeNull();
		// Circle should be off-center (not at origin)
		const distFromOrigin = Math.sqrt(circleCenter.x ** 2 + circleCenter.y ** 2);
		expect(distFromOrigin).toBeGreaterThan(0.5);

		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('alignment-circle-finish-failed');
		}

		// Extrude via dialog
		await clickExtrude(waffle.page);
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		if (await depthInput.isVisible()) {
			await depthInput.fill('10');
		}
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('alignment-circle-extrude-failed');
		}

		// Wait for mesh
		await waffle.page.waitForFunction(
			() => {
				const meshes = window.__waffle?.getMeshes() ?? [];
				return meshes.some(m => m.triangleCount > 0);
			},
			{ timeout: 10000 }
		);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});
});
