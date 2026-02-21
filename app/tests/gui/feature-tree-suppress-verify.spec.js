/**
 * Feature tree suppress/unsuppress tests with model state verification.
 *
 * Goes beyond CSS class checking to verify that suppressing features
 * actually affects the 3D model (mesh presence/absence).
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
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
} from './helpers/state.js';

const FEATURE_ITEM = '.tree-item:not(.origin-item)';

/**
 * Helper: create a sketch with a rectangle, finish it, then extrude.
 * Results in 2 features: Sketch + Extrude with a visible mesh.
 */
async function createSketchAndExtrude(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -50, -50, 50, 50);
	await waitForEntityCount(waffle.page, 4, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 5000);

	await clickExtrude(waffle.page);
	const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
	await depthInput.fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 5000);
	await waitForMeshWithGeometry(waffle.page, 5000);
}

test.describe('suppress/unsuppress with model verification', () => {
	test('suppress extrude removes mesh', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Verify mesh exists before suppress
		const hasMeshBefore = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshBefore).toBe(true);

		// Right-click the extrude feature (index 1) to open context menu
		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);
		await extrudeItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		// Click Suppress
		await waffle.page.locator('[data-testid="ft-ctx-suppress"]').click();
		await waffle.page.waitForTimeout(500);

		// Extrude should have suppressed class
		await expect(extrudeItem).toHaveClass(/suppressed/);

		// Mesh should be gone (extrude is suppressed, no 3D body)
		const hasMeshAfter = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshAfter).toBe(false);
	});

	test('unsuppress extrude restores mesh', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const extrudeItem = waffle.page.locator(FEATURE_ITEM).nth(1);

		// Suppress first
		await extrudeItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);
		await waffle.page.locator('[data-testid="ft-ctx-suppress"]').click();
		await waffle.page.waitForTimeout(500);

		// Verify suppressed
		await expect(extrudeItem).toHaveClass(/suppressed/);
		const hasMeshSuppressed = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshSuppressed).toBe(false);

		// Right-click again to unsuppress
		await extrudeItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);

		// Context menu should show "Unsuppress"
		const suppressBtn = waffle.page.locator('[data-testid="ft-ctx-suppress"]');
		await expect(suppressBtn).toContainText('Unsuppress');
		await suppressBtn.click();
		await waffle.page.waitForTimeout(500);

		// Suppressed class should be removed
		const classes = await extrudeItem.getAttribute('class');
		expect(classes).not.toContain('suppressed');

		// Mesh should be restored
		await waitForMeshWithGeometry(waffle.page, 5000);
		const hasMeshRestored = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshRestored).toBe(true);
	});

	test('suppress sketch suppresses dependent extrude', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Verify mesh exists
		const hasMeshBefore = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshBefore).toBe(true);

		// Suppress the sketch (index 0) — the extrude depends on it
		const sketchItem = waffle.page.locator(FEATURE_ITEM).nth(0);
		await sketchItem.click({ button: 'right' });
		await waffle.page.waitForTimeout(200);
		await waffle.page.locator('[data-testid="ft-ctx-suppress"]').click();
		await waffle.page.waitForTimeout(500);

		// Sketch should have suppressed class
		await expect(sketchItem).toHaveClass(/suppressed/);

		// Mesh should be gone (no sketch means extrude has no profile)
		const hasMeshAfter = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshAfter).toBe(false);
	});
});
