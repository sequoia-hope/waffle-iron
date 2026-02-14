/**
 * Shared geometry creation and face interaction helpers for selection tests.
 *
 * Geometry is created programmatically via __waffle API (acceptable hybrid pattern
 * per skeptic report). All selection interactions use real mouse events through
 * the full picking pipeline:
 *   page.mouse.click() → DOM pointer events → Threlte interactivity() →
 *   THREE.js raycaster → findFaceRef() → selectRef()
 *
 * projectFaceCentroids() is read-only coordinate discovery, not selection bypass.
 */
import { getCanvasBounds } from './canvas.js';

/**
 * Create an extruded box via the __waffle API.
 * This is geometry setup — the tests focus on selection via real GUI events.
 * @param {import('@playwright/test').Page} page
 */
export async function createExtrudedBox(page) {
	// Enter sketch mode on XY plane
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(200);

	// Add square entities: 4 points + 4 lines (60×60 square → cube after extrude)
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

	// Finish sketch — sends solved positions + profiles to engine
	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1,
		{ timeout: 10000 }
	);
	await page.waitForTimeout(200);

	// Show extrude dialog and apply (depth 60 → 60×60×60 cube)
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(() => window.__waffle.applyExtrude(60, 0, false));

	// Wait for extrude feature and mesh
	await page.waitForFunction(
		() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 2,
		{ timeout: 10000 }
	);
	await page.waitForFunction(
		() => (window.__waffle?.getMeshes() ?? []).some(m => m.triangleCount > 0),
		{ timeout: 10000 }
	);

	// Let rendering settle
	await page.waitForTimeout(500);
}

/**
 * Get visible face centroids projected to screen coordinates via projectFaceCentroids.
 * This is read-only coordinate discovery, not selection bypass.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<Array<{geomRef: any, screenX: number, screenY: number}>>}
 */
export async function getVisibleFaces(page) {
	return page.evaluate(() => window.__waffle?.projectFaceCentroids?.() ?? []);
}

/**
 * Click on a face centroid position via real mouse event.
 * The click goes through the full picking pipeline (DOM → Threlte → raycaster → selectRef).
 * @param {import('@playwright/test').Page} page
 * @param {{ screenX: number, screenY: number }} faceInfo
 * @param {{ shift?: boolean }} [options]
 */
export async function clickFace(page, faceInfo, options = {}) {
	if (options.shift) {
		// Use keyboard.down/up for shift — Threlte events read shiftKey from the native DOM event,
		// which requires the actual Shift key to be held, not Playwright's modifiers option.
		await page.keyboard.down('Shift');
		await page.mouse.click(faceInfo.screenX, faceInfo.screenY);
		await page.keyboard.up('Shift');
	} else {
		await page.mouse.click(faceInfo.screenX, faceInfo.screenY);
	}
	await page.waitForTimeout(200);
}

/**
 * Find two face centroid positions that select DIFFERENT faces when clicked.
 * Returns [face1, face2] or null if only one distinct face is selectable.
 * @param {import('@playwright/test').Page} page
 * @param {Array<{geomRef: any, screenX: number, screenY: number}>} faces
 * @returns {Promise<[{screenX: number, screenY: number}, {screenX: number, screenY: number}] | null>}
 */
export async function findTwoDistinctFaces(page, faces) {
	if (faces.length < 2) return null;

	// Click first face and get its ref
	await page.mouse.click(faces[0].screenX, faces[0].screenY);
	await page.waitForTimeout(200);
	const firstSelected = await page.evaluate(() => {
		const refs = window.__waffle.getSelectedRefs();
		return refs.length > 0 ? JSON.stringify(refs[0]) : null;
	});
	if (!firstSelected) return null;

	// Try each other face centroid to find one that selects a different ref
	for (let i = 1; i < faces.length; i++) {
		await page.mouse.click(faces[i].screenX, faces[i].screenY);
		await page.waitForTimeout(200);
		const secondSelected = await page.evaluate(() => {
			const refs = window.__waffle.getSelectedRefs();
			return refs.length > 0 ? JSON.stringify(refs[0]) : null;
		});
		if (secondSelected && secondSelected !== firstSelected) {
			// Clear selection before returning
			await page.evaluate(() => window.__waffle.clearSelection());
			return [faces[0], faces[i]];
		}
	}

	// Clear selection
	await page.evaluate(() => window.__waffle.clearSelection());
	return null;
}

/**
 * Click in empty space (far from geometry) to clear selection via real mouse event.
 * @param {import('@playwright/test').Page} page
 */
export async function clickEmpty(page) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	// Click near top-left corner of canvas — far from centered geometry
	await page.mouse.click(bounds.x + 15, bounds.y + 15);
	await page.waitForTimeout(200);
}

/**
 * Perform a box-select drag from start to end (screen coordinates).
 * Uses multi-step mouse move to trigger box selection behavior.
 * @param {import('@playwright/test').Page} page
 * @param {number} startX
 * @param {number} startY
 * @param {number} endX
 * @param {number} endY
 */
export async function dragBox(page, startX, startY, endX, endY) {
	await page.mouse.move(startX, startY);
	await page.mouse.down();
	const steps = 10;
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(
			startX + (endX - startX) * t,
			startY + (endY - startY) * t
		);
	}
	await page.mouse.up();
	await page.waitForTimeout(300);
}
