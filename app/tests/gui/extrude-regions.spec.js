/**
 * Extrude panel region list tests — verifies the non-modal extrude panel's
 * region list section: auto-population, region display, add/remove operations,
 * empty state, and persistence across field changes.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle, orbitDrag } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle and finish it.
 */
async function sketchRectangle(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);

	try {
		await waitForEntityCount(waffle.page, 8, 5000);
	} catch {
		await waffle.dumpState('region-sketch-draw-failed');
	}

	await clickFinishSketch(waffle.page);

	try {
		await waitForFeatureCount(waffle.page, 1, 10000);
	} catch {
		await waffle.dumpState('region-sketch-finish-failed');
	}
}

test.describe('extrude panel layout', () => {
	test('panel is non-modal — has absolute position and canvas remains interactive', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Verify the panel uses absolute positioning (non-modal)
		const panel = waffle.page.locator('[data-testid="extrude-dialog"]');
		await expect(panel).toBeVisible();

		const position = await panel.evaluate(el => {
			return window.getComputedStyle(el).position;
		});
		expect(position).toBe('absolute');

		// Verify canvas orbit still works while panel is open:
		// capture camera state, orbit, confirm camera changed
		const cameraBefore = await waffle.page.evaluate(() => {
			const cam = window.__waffle.getCameraState();
			return cam;
		});

		await orbitDrag(waffle.page, -100, 0, 100, 50);

		const cameraAfter = await waffle.page.evaluate(() => {
			const cam = window.__waffle.getCameraState();
			return cam;
		});

		// Camera should have moved (orbit changes position or rotation)
		const posChanged =
			cameraBefore?.position?.[0] !== cameraAfter?.position?.[0] ||
			cameraBefore?.position?.[1] !== cameraAfter?.position?.[1] ||
			cameraBefore?.position?.[2] !== cameraAfter?.position?.[2];
		expect(posChanged).toBe(true);
	});
});

test.describe('extrude region list', () => {
	test('region list section visible after opening extrude', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const regionList = waffle.page.locator('[data-testid="extrude-regions"]');
		await expect(regionList).toBeVisible();
	});

	test('region auto-populated after sketch', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// First region should be auto-populated from the last sketch
		const region0 = waffle.page.locator('[data-testid="extrude-region-0"]');
		await expect(region0).toBeVisible();
	});

	test('region shows sketch name and profile', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const regionLabel = waffle.page.locator('[data-testid="extrude-region-0"] .region-label');
		await expect(regionLabel).toBeVisible();

		const text = await regionLabel.textContent();
		expect(text).toContain('Sketch');
		expect(text).toContain('Profile 1');
	});

	test('remove region button works', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Confirm region-0 exists
		const region0 = waffle.page.locator('[data-testid="extrude-region-0"]');
		await expect(region0).toBeVisible();

		// Click the remove button on region 0
		await waffle.page.locator('[data-testid="extrude-region-0"] .region-remove').click();
		await waffle.page.waitForTimeout(200);

		// Region 0 should be gone
		await expect(waffle.page.locator('[data-testid="extrude-region-0"]')).not.toBeVisible();

		// Empty state should appear
		const emptyState = waffle.page.locator('.region-empty');
		await expect(emptyState).toBeVisible();
	});

	test('getExtrudeRegions API returns region array', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		const regions = await waffle.page.evaluate(() => window.__waffle.getExtrudeRegions());
		expect(Array.isArray(regions)).toBe(true);
		expect(regions.length).toBeGreaterThanOrEqual(1);
		expect(regions[0]).toHaveProperty('sketchId');
		expect(regions[0]).toHaveProperty('sketchName');
		expect(regions[0]).toHaveProperty('profileIndex');
	});

	test('addExtrudeRegion via API after removing region', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Get the sketch info before removing
		const regionsBefore = await waffle.page.evaluate(() => window.__waffle.getExtrudeRegions());
		const sketchId = regionsBefore[0].sketchId;
		const sketchName = regionsBefore[0].sketchName;

		// Remove region 0
		await waffle.page.evaluate(() => window.__waffle.removeExtrudeRegion(0));
		await waffle.page.waitForTimeout(200);

		// Confirm region is gone
		await expect(waffle.page.locator('[data-testid="extrude-region-0"]')).not.toBeVisible();

		// Re-add via API
		await waffle.page.evaluate(
			({ id, name }) => window.__waffle.addExtrudeRegion(id, name, 0),
			{ id: sketchId, name: sketchName }
		);
		await waffle.page.waitForTimeout(200);

		// Region item should reappear
		const region0 = waffle.page.locator('[data-testid="extrude-region-0"]');
		await expect(region0).toBeVisible();
	});

	test('empty state when no regions', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Remove all regions via API
		await waffle.page.evaluate(() => {
			const regions = window.__waffle.getExtrudeRegions();
			for (let i = regions.length - 1; i >= 0; i--) {
				window.__waffle.removeExtrudeRegion(i);
			}
		});
		await waffle.page.waitForTimeout(200);

		// Empty state should be visible with the correct text
		const emptyState = waffle.page.locator('.region-empty');
		await expect(emptyState).toBeVisible();
		await expect(emptyState).toHaveText('No regions selected');
	});

	test('region list persists across field changes', async ({ waffle }) => {
		await sketchRectangle(waffle);
		await clickExtrude(waffle.page);

		// Confirm region exists
		const region0 = waffle.page.locator('[data-testid="extrude-region-0"]');
		await expect(region0).toBeVisible();

		// Change depth mode to Through All
		await waffle.page.locator('[data-testid="extrude-depth-mode"]').selectOption('ThroughAll');
		await waffle.page.waitForTimeout(100);

		// Region should still be present
		await expect(region0).toBeVisible();

		// Change back to Blind
		await waffle.page.locator('[data-testid="extrude-depth-mode"]').selectOption('Blind');
		await waffle.page.waitForTimeout(100);

		// Region should still be present
		await expect(region0).toBeVisible();

		// Change second direction
		await waffle.page.locator('[data-testid="extrude-second-dir"]').selectOption('Symmetric');
		await waffle.page.waitForTimeout(100);

		// Region should still be present
		await expect(region0).toBeVisible();
	});

	test('extrude applies with correct region and creates mesh', async ({ waffle }) => {
		await sketchRectangle(waffle);
		const featuresBefore = await getFeatureCount(waffle.page);

		await clickExtrude(waffle.page);

		// Confirm region is present
		await expect(waffle.page.locator('[data-testid="extrude-region-0"]')).toBeVisible();

		// Set depth and apply
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('15');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Dialog should close
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Feature count should increase by 1
		try {
			await waitForFeatureCount(waffle.page, featuresBefore + 1, 10000);
		} catch {
			await waffle.dumpState('region-extrude-apply-failed');
		}

		const featuresAfter = await getFeatureCount(waffle.page);
		expect(featuresAfter).toBe(featuresBefore + 1);

		// Mesh with geometry should exist
		try {
			await waitForMeshWithGeometry(waffle.page, 10000);
		} catch {
			await waffle.dumpState('region-extrude-mesh-failed');
		}

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});
});
