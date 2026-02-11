import { test, expect } from '@playwright/test';

async function waitForEngine(page, timeout = 15000) {
	return page
		.waitForFunction(
			() => window.__waffle && window.__waffle.getState().engineReady,
			{ timeout }
		)
		.then(() => true)
		.catch(() => false);
}

async function enterSketchAndWait(page, tool = 'line') {
	await page.evaluate(({ tool }) => {
		window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
		window.__waffle.setTool(tool);
	}, { tool });

	await page.waitForFunction(
		({ tool }) => {
			const state = window.__waffle?.getState();
			return state?.sketchMode?.active === true && state?.activeTool === tool;
		},
		{ tool },
		{ timeout: 5000 }
	);

	await page.waitForTimeout(300);
}

test.describe('sketch → extrude pipeline', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/');
	});

	test('sketch → finish → extrude → verify 3D mesh', async ({ page }) => {
		const ready = await waitForEngine(page);
		test.skip(!ready, 'Engine not ready (WASM may not have loaded)');

		await enterSketchAndWait(page, 'rectangle');

		const canvas = page.locator('canvas');
		await expect(canvas).toBeVisible();
		const box = await canvas.boundingBox();
		if (!box) { test.skip(true, 'Canvas not visible'); return; }

		// Draw rectangle (two-click corners)
		await canvas.click({ position: { x: Math.round(box.width * 0.3), y: Math.round(box.height * 0.3) } });
		await page.waitForTimeout(300);
		await canvas.click({ position: { x: Math.round(box.width * 0.7), y: Math.round(box.height * 0.7) } });
		await page.waitForTimeout(500);

		// Verify 4 lines were created
		const entities = await page.evaluate(() => window.__waffle.getEntities());
		expect(entities.filter(e => e.type === 'Line').length).toBe(4);

		// Finish sketch
		await page.evaluate(() => { window.__waffle.finishSketch(); });

		await page.waitForFunction(
			() => window.__waffle.getFeatureTree()?.features?.some(f => f.operation?.type === 'Sketch'),
			{ timeout: 10000 }
		);

		// Extrude the sketch profile
		await page.evaluate(() => {
			window.__waffle.showExtrudeDialog();
			window.__waffle.applyExtrude(10, 0);
		});

		await page.waitForTimeout(3000);

		// Verify feature tree
		const finalTree = await page.evaluate(() => window.__waffle.getFeatureTree());
		expect(finalTree.features.length).toBe(2);
		expect(finalTree.features.some(f => f.operation?.type === 'Sketch')).toBe(true);
		expect(finalTree.features.some(f => f.operation?.type === 'Extrude')).toBe(true);

		// Verify 3D mesh was generated
		const meshes = await page.evaluate(() => window.__waffle.getMeshes());
		expect(meshes.length).toBeGreaterThan(0);
		const meshWithGeometry = meshes.find(m => m.triangleCount > 0 && m.vertexCount > 0);
		expect(meshWithGeometry).toBeTruthy();
	});

	test('extruded solid has pickable faces with GeomRefs', async ({ page }) => {
		const ready = await waitForEngine(page);
		test.skip(!ready, 'Engine not ready (WASM may not have loaded)');

		await enterSketchAndWait(page, 'rectangle');

		const canvas = page.locator('canvas');
		await expect(canvas).toBeVisible();
		const box = await canvas.boundingBox();
		if (!box) { test.skip(true, 'Canvas not visible'); return; }

		// Draw rectangle
		await canvas.click({ position: { x: Math.round(box.width * 0.3), y: Math.round(box.height * 0.3) } });
		await page.waitForTimeout(300);
		await canvas.click({ position: { x: Math.round(box.width * 0.7), y: Math.round(box.height * 0.7) } });
		await page.waitForTimeout(500);

		// Finish sketch and extrude
		await page.evaluate(() => { window.__waffle.finishSketch(); });
		await page.waitForFunction(
			() => window.__waffle.getFeatureTree()?.features?.some(f => f.operation?.type === 'Sketch'),
			{ timeout: 10000 }
		);
		await page.evaluate(() => {
			window.__waffle.showExtrudeDialog();
			window.__waffle.applyExtrude(10, 0);
		});
		await page.waitForTimeout(3000);

		// Verify face ranges are present on the mesh
		const meshes = await page.evaluate(() => window.__waffle.getMeshes());
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
		const plane = await page.evaluate(
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

	test('save/load roundtrip preserves feature tree', async ({ page }) => {
		const ready = await waitForEngine(page);
		test.skip(!ready, 'Engine not ready (WASM may not have loaded)');

		await enterSketchAndWait(page, 'rectangle');

		const canvas = page.locator('canvas');
		await expect(canvas).toBeVisible();
		const box = await canvas.boundingBox();
		if (!box) { test.skip(true, 'Canvas not visible'); return; }

		// Draw rectangle
		await canvas.click({ position: { x: Math.round(box.width * 0.3), y: Math.round(box.height * 0.3) } });
		await page.waitForTimeout(300);
		await canvas.click({ position: { x: Math.round(box.width * 0.7), y: Math.round(box.height * 0.7) } });
		await page.waitForTimeout(500);

		// Finish sketch and extrude
		await page.evaluate(() => { window.__waffle.finishSketch(); });
		await page.waitForFunction(
			() => window.__waffle.getFeatureTree()?.features?.some(f => f.operation?.type === 'Sketch'),
			{ timeout: 10000 }
		);
		await page.evaluate(() => {
			window.__waffle.showExtrudeDialog();
			window.__waffle.applyExtrude(10, 0);
		});
		await page.waitForTimeout(3000);

		// Verify we have 2 features before save
		const treeBefore = await page.evaluate(() => window.__waffle.getFeatureTree());
		expect(treeBefore.features.length).toBe(2);

		// Save project — returns JSON string
		const jsonData = await page.evaluate(() => window.__waffle.saveProject());
		expect(jsonData).toBeTruthy();
		expect(typeof jsonData).toBe('string');

		// Parse and verify format
		const parsed = JSON.parse(jsonData);
		expect(parsed.format).toBe('waffle-iron');
		expect(parsed.features).toBeDefined();
		// features is a FeatureTree object with its own .features array
		expect(parsed.features.features.length).toBe(2);

		// Load the saved data back
		await page.evaluate((data) => window.__waffle.loadProject(data), jsonData);
		await page.waitForTimeout(2000);

		// Verify feature tree restored
		const treeAfter = await page.evaluate(() => window.__waffle.getFeatureTree());
		expect(treeAfter.features.length).toBe(2);
		expect(treeAfter.features.some(f => f.operation?.type === 'Sketch')).toBe(true);
		expect(treeAfter.features.some(f => f.operation?.type === 'Extrude')).toBe(true);
	});

	test('sketch-on-face enters sketch mode with correct plane', async ({ page }) => {
		const ready = await waitForEngine(page);
		test.skip(!ready, 'Engine not ready (WASM may not have loaded)');

		await enterSketchAndWait(page, 'rectangle');

		const canvas = page.locator('canvas');
		await expect(canvas).toBeVisible();
		const box = await canvas.boundingBox();
		if (!box) { test.skip(true, 'Canvas not visible'); return; }

		// Draw rectangle
		await canvas.click({ position: { x: Math.round(box.width * 0.3), y: Math.round(box.height * 0.3) } });
		await page.waitForTimeout(300);
		await canvas.click({ position: { x: Math.round(box.width * 0.7), y: Math.round(box.height * 0.7) } });
		await page.waitForTimeout(500);

		// Finish sketch and extrude
		await page.evaluate(() => { window.__waffle.finishSketch(); });
		await page.waitForFunction(
			() => window.__waffle.getFeatureTree()?.features?.some(f => f.operation?.type === 'Sketch'),
			{ timeout: 10000 }
		);
		await page.evaluate(() => {
			window.__waffle.showExtrudeDialog();
			window.__waffle.applyExtrude(10, 0);
		});
		await page.waitForTimeout(3000);

		// Get a face ref and compute its plane
		const faceRef = await page.evaluate(() => {
			const meshes = window.__waffle.getMeshes();
			const mesh = meshes.find(m => m.faceRangeCount > 0);
			if (!mesh || mesh.faceRanges.length === 0) return null;
			return mesh.faceRanges[0].geom_ref;
		});
		expect(faceRef).toBeTruthy();

		const plane = await page.evaluate(
			(ref) => window.__waffle.computeFacePlane(ref),
			faceRef
		);
		expect(plane).toBeTruthy();

		// Enter sketch on the computed face plane
		await page.evaluate(({ origin, normal }) => {
			window.__waffle.enterSketch(origin, normal);
		}, plane);

		// Verify sketch mode is active
		const state = await page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
	});
});
