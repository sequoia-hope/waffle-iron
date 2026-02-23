/**
 * Sketch E2E flow tests — complete sketch lifecycle:
 * start sketch → draw → finish sketch → verify feature in tree.
 *
 * These tests detect the WASM "unreachable" panic caused by
 * std::time::Instant::now() in Rust code panicking on WASM.
 * After the Rust fix is applied, these tests should pass.
 *
 * Uses clickSketch() (standard test helper) to enter sketch mode.
 * __waffle is only used for state VERIFICATION, not for triggering actions.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickLine,
	clickRectangle,
	clickCircle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawLine, drawRectangle, drawCircle } from './helpers/canvas.js';
import {
	isSketchActive,
	getFeatureTree,
	hasFeatureOfType,
	hasMeshWithGeometry,
	waitForEntityCount,
	waitForFeatureCount,
	waitForMeshWithGeometry,
} from './helpers/state.js';

test.describe('sketch E2E flow', () => {
	test('sketch rectangle → finish → feature in tree', async ({ waffle }) => {
		const { page } = waffle;

		// 1. Enter sketch mode on front plane
		await clickSketch(page);

		// 2. Select rectangle tool
		await clickRectangle(page);

		// 3. Draw rectangle via real canvas clicks
		await drawRectangle(page, -80, -60, 80, 60);

		// 4. Wait for 8 entities (4 points + 4 lines)
		await waitForEntityCount(page, 8, 5000);

		// 5. Click Finish Sketch button
		await clickFinishSketch(page);

		// 6. Wait for feature count to be 1
		await waitForFeatureCount(page, 1, 10000);

		// 7. Verify feature tree has a 'Sketch' feature
		expect(await hasFeatureOfType(page, 'Sketch')).toBe(true);

		// 8. Verify sketch mode is inactive
		expect(await isSketchActive(page)).toBe(false);
	});

	test('sketch circle → finish → feature in tree', async ({ waffle }) => {
		const { page } = waffle;

		// 1. Enter sketch mode
		await clickSketch(page);

		// 2. Select circle tool
		await clickCircle(page);

		// 3. Draw circle (center + edge click)
		await drawCircle(page, 0, 0, 50, 0);

		// 4. Wait for 2 entities (1 point + 1 circle)
		await waitForEntityCount(page, 2, 5000);

		// 5. Click Finish Sketch
		await clickFinishSketch(page);

		// 6. Wait for feature count to be 1
		await waitForFeatureCount(page, 1, 10000);

		// 7. Verify feature tree has a 'Sketch' feature
		expect(await hasFeatureOfType(page, 'Sketch')).toBe(true);

		// 8. Verify sketch mode is inactive
		expect(await isSketchActive(page)).toBe(false);
	});

	test('sketch line → finish → feature in tree', async ({ waffle }) => {
		const { page } = waffle;

		// 1. Enter sketch mode
		await clickSketch(page);

		// 2. Select line tool
		await clickLine(page);

		// 3. Draw a line (two clicks)
		await drawLine(page, -60, -40, 60, 40);

		// 4. Wait for 3 entities (2 points + 1 line)
		await waitForEntityCount(page, 3, 5000);

		// 5. Click Finish Sketch
		await clickFinishSketch(page);

		// 6. Wait for feature count to be 1
		await waitForFeatureCount(page, 1, 10000);

		// 7. Verify feature tree has a 'Sketch' feature
		expect(await hasFeatureOfType(page, 'Sketch')).toBe(true);

		// 8. Verify sketch mode is inactive
		expect(await isSketchActive(page)).toBe(false);
	});

	test('sketch rectangle → finish → extrude → 3D mesh', async ({ waffle }) => {
		const { page } = waffle;

		// 1. Enter sketch, draw rectangle, finish
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		// 2. Click Extrude button
		await clickExtrude(page);

		// 3. Fill depth=10 in the extrude dialog
		const depthInput = page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');

		// 4. Click Apply
		await page.locator('[data-testid="extrude-apply"]').click();

		// 5. Wait for feature count 2
		await waitForFeatureCount(page, 2, 10000);

		// 6. Verify both Sketch and Extrude features exist
		const tree = await getFeatureTree(page);
		expect(tree.features.length).toBe(2);
		expect(await hasFeatureOfType(page, 'Sketch')).toBe(true);
		expect(await hasFeatureOfType(page, 'Extrude')).toBe(true);

		// 7. Verify 3D mesh was generated
		await waitForMeshWithGeometry(page, 10000);
		expect(await hasMeshWithGeometry(page)).toBe(true);
	});
});
