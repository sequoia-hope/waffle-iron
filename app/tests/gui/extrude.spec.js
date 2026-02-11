/**
 * Extrude workflow — dialog, apply, cancel, feature creation.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getMeshes,
} from './helpers/state.js';

/**
 * Helper: complete a sketch with a rectangle.
 */
async function sketchRectangle(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);

	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('extrude-sketch-draw-failed');
	}

	await clickFinishSketch(waffle.page);

	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('extrude-sketch-finish-failed');
	}
}

test.describe('extrude workflow via GUI', () => {
	test('after finishing sketch, clicking Extrude shows dialog', async ({ waffle }) => {
		await sketchRectangle(waffle);

		await clickExtrude(waffle.page);

		const dialog = waffle.page.locator('[data-testid="extrude-dialog"]');
		await expect(dialog).toBeVisible();

		// Depth input should be visible with default value
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await expect(depthInput).toBeVisible();

		// Apply and Cancel buttons should be visible
		await expect(waffle.page.locator('[data-testid="extrude-apply"]')).toBeVisible();
		await expect(waffle.page.locator('[data-testid="extrude-cancel"]')).toBeVisible();
	});

	test('extrude dialog Cancel closes without creating feature', async ({ waffle }) => {
		await sketchRectangle(waffle);

		const featuresBefore = await getFeatureCount(waffle.page);

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-cancel"]').click();

		// Dialog should be gone
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Feature count should not have changed
		const featuresAfter = await getFeatureCount(waffle.page);
		expect(featuresAfter).toBe(featuresBefore);
	});

	test('extrude dialog Apply creates Extrude feature', async ({ waffle }) => {
		await sketchRectangle(waffle);

		await clickExtrude(waffle.page);

		// Set depth value
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');

		// Click Apply
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Wait for feature to be added
		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('extrude-apply-failed');
		}

		// Feature tree should have Sketch + Extrude
		const hasSketch = await hasFeatureOfType(waffle.page, 'Sketch');
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasSketch).toBe(true);
		expect(hasExtrude).toBe(true);
	});

	test('extrude creates 3D mesh with triangles', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Wait for mesh to appear
		try {
			await waffle.page.waitForFunction(
				() => {
					const meshes = window.__waffle?.getMeshes() ?? [];
					return meshes.some(m => m.triangleCount > 0);
				},
				{ timeout: 10000 }
			);
		} catch {
			await waffle.dumpState('extrude-mesh-failed');
		}

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);

		const meshes = await getMeshes(waffle.page);
		const extrudeMesh = meshes.find(m => m.triangleCount > 0);
		expect(extrudeMesh).toBeDefined();
		expect(extrudeMesh.triangleCount).toBeGreaterThan(0);
	});

	test('Enter key in extrude dialog applies', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('5');

		// Press Enter to apply
		await waffle.page.keyboard.press('Enter');

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	});

	test('Escape key in extrude dialog cancels', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Press Escape to cancel
		await waffle.page.keyboard.press('Escape');

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	});
});
