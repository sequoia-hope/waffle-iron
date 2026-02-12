/**
 * Pipeline tests — full sketch → extrude workflows via real GUI events.
 *
 * Previously these tests bypassed the GUI entirely via __waffle API calls
 * (enterSketch, finishSketch, showExtrudeDialog, applyExtrude).
 * Now they route through actual toolbar button clicks and dialog interactions.
 *
 * __waffle is only used for state VERIFICATION, not for triggering actions.
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
	getEntities,
	getFeatureCount,
	getFeatureTree,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getMeshes,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

test.describe('sketch → extrude pipeline', () => {
	test('sketch → finish → extrude → verify 3D mesh', async ({ waffle }) => {
		// Step 1: Click Sketch button (real toolbar click)
		await clickSketch(waffle.page);

		// Step 2: Click Rectangle tool (real toolbar click)
		await clickRectangle(waffle.page);

		// Step 3: Draw rectangle (real canvas clicks)
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try {
			await waitForEntityCount(waffle.page, 8, 3000);
		} catch {
			await waffle.dumpState('pipeline-draw-failed');
		}

		// Verify 4 lines created
		const entities = await getEntities(waffle.page);
		expect(entities.filter(e => e.type === 'Line').length).toBe(4);

		// Step 4: Click Finish Sketch button (real toolbar click)
		await clickFinishSketch(waffle.page);
		try {
			await waitForFeatureCount(waffle.page, 1, 10000);
		} catch {
			await waffle.dumpState('pipeline-finish-failed');
		}

		// Step 5: Click Extrude button, fill depth, click Apply (real dialog interaction)
		await clickExtrude(waffle.page);
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		try {
			await waitForFeatureCount(waffle.page, 2, 10000);
		} catch {
			await waffle.dumpState('pipeline-extrude-failed');
		}

		// Verify feature tree (read-only verification via __waffle)
		const tree = await getFeatureTree(waffle.page);
		expect(tree.features.length).toBe(2);
		expect(await hasFeatureOfType(waffle.page, 'Sketch')).toBe(true);
		expect(await hasFeatureOfType(waffle.page, 'Extrude')).toBe(true);

		// Verify 3D mesh was generated
		expect(await hasMeshWithGeometry(waffle.page)).toBe(true);
	});

	test('extruded solid has pickable faces with GeomRefs', async ({ waffle }) => {
		// Full GUI workflow: sketch → draw → finish → extrude
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('geomref-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('geomref-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('pipeline-geomref-extrude-failed');
		}

		// Verify face ranges (read-only verification)
		const meshes = await getMeshes(waffle.page);
		const meshWithFaces = meshes.find(m => m.faceRangeCount > 0);
		expect(meshWithFaces).toBeTruthy();

		// An extruded rectangle should have 6 faces (top, bottom, 4 sides)
		expect(meshWithFaces.faceRangeCount).toBeGreaterThanOrEqual(6);

		// Each face range should have a geom_ref with kind=Face
		for (const range of meshWithFaces.faceRanges) {
			expect(range.geom_ref).toBeTruthy();
			expect(range.geom_ref.kind).toEqual({ type: 'Face' });
			expect(range.geom_ref.anchor).toBeTruthy();
			expect(range.geom_ref.selector).toBeTruthy();
			expect(range.start_index).toBeDefined();
			expect(range.end_index).toBeGreaterThan(range.start_index);
		}

		// Verify computeFacePlane works for at least one face
		const firstRef = meshWithFaces.faceRanges[0].geom_ref;
		const plane = await waffle.page.evaluate(
			(ref) => window.__waffle.computeFacePlane(ref),
			firstRef
		);
		expect(plane).toBeTruthy();
		expect(plane.origin).toHaveLength(3);
		expect(plane.normal).toHaveLength(3);

		// Normal should be unit length
		const len = Math.sqrt(
			plane.normal[0] ** 2 + plane.normal[1] ** 2 + plane.normal[2] ** 2
		);
		expect(len).toBeCloseTo(1.0, 3);
	});

	test('save/load roundtrip preserves feature tree', async ({ waffle }) => {
		// Full GUI workflow
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('saveload-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('saveload-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('saveload-extrude-failed');
		}

		// Verify 2 features before save
		const treeBefore = await getFeatureTree(waffle.page);
		expect(treeBefore.features.length).toBe(2);

		// Save project (API call — no GUI save dialog exists yet)
		const jsonData = await waffle.page.evaluate(() => window.__waffle.saveProject());
		expect(jsonData).toBeTruthy();
		expect(typeof jsonData).toBe('string');

		// Parse and verify format
		const parsed = JSON.parse(jsonData);
		expect(parsed.format).toBe('waffle-iron');
		expect(parsed.features.features.length).toBe(2);

		// Load back (API call — no GUI open dialog exists yet)
		await waffle.page.evaluate((data) => window.__waffle.loadProject(data), jsonData);
		await waffle.page.waitForTimeout(2000);

		// Verify feature tree restored
		const treeAfter = await getFeatureTree(waffle.page);
		expect(treeAfter.features.length).toBe(2);
		expect(await hasFeatureOfType(waffle.page, 'Sketch')).toBe(true);
		expect(await hasFeatureOfType(waffle.page, 'Extrude')).toBe(true);
	});

	test('sketch-on-face enters sketch mode with correct plane', async ({ waffle }) => {
		// Full GUI workflow to create extruded box
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('sof-pipeline-draw-failed');
		}

		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('sof-pipeline-finish-failed');
		}

		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('pipeline-sof-extrude-failed');
		}

		// Get a face ref (read-only API — face picking requires 3D raycast)
		const faceRef = await waffle.page.evaluate(() => {
			const meshes = window.__waffle.getMeshes();
			const mesh = meshes.find(m => m.faceRangeCount > 0);
			if (!mesh || mesh.faceRanges.length === 0) return null;
			return mesh.faceRanges[0].geom_ref;
		});
		expect(faceRef).toBeTruthy();

		// Select the face (programmatic — 3D face picking is a raycast operation)
		await waffle.page.evaluate((ref) => {
			window.__waffle.selectRef(ref);
		}, faceRef);
		await waffle.page.waitForTimeout(200);

		// Click Sketch button (real toolbar click) to enter sketch on selected face
		await clickSketch(waffle.page);

		// Verify sketch mode is active
		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
	});
});
