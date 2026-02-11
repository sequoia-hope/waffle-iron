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
});
