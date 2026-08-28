/**
 * Export workflow tests — save/load project, STL/STEP export.
 * Covers: saveProject, loadProject, exportStl, exportStep, graceful empty-model handling.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Helper: create a sketch + extrude to get a model with mesh geometry.
 */
async function createSketchAndExtrude(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('export-sketch-draw-failed');
	}
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('export-sketch-finish-failed');
	}
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
		await waffle.dumpState('export-extrude-failed');
	}
}

test.describe('export workflow', () => {
	test('saveProject returns valid JSON with features', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		const jsonStr = await waffle.page.evaluate(() => window.__waffle.saveProject());
		expect(jsonStr).toBeTruthy();

		const parsed = JSON.parse(jsonStr);
		// Project JSON should have a recognizable structure
		expect(parsed).toBeDefined();
		const hasFeatures = 'features' in parsed || 'version' in parsed || 'tree' in parsed;
		expect(hasFeatures).toBe(true);
	});

	test('loadProject restores features and mesh', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Save the current project
		const savedJson = await waffle.page.evaluate(() => window.__waffle.saveProject());
		expect(savedJson).toBeTruthy();

		// Load the saved project
		await waffle.page.evaluate(json => window.__waffle.loadProject(json), savedJson);

		// Wait for rebuild
		await waffle.page.waitForTimeout(2000);

		const featureCount = await getFeatureCount(waffle.page);
		expect(featureCount).toBeGreaterThanOrEqual(2);

		const hasMesh = await hasMeshWithGeometry(waffle.page);
		expect(hasMesh).toBe(true);
	});

	test('load on fresh navigation restores state', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Save project
		const savedJson = await waffle.page.evaluate(() => window.__waffle.saveProject());
		expect(savedJson).toBeTruthy();

		// Navigate to fresh page
		await waffle.page.goto('/');

		// Wait for engine to be ready again
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.engineReady === true,
			{ timeout: 30000 }
		);

		// Load the saved project
		await waffle.page.evaluate(json => window.__waffle.loadProject(json), savedJson);

		// Wait for rebuild
		await waffle.page.waitForTimeout(2000);

		const featureCount = await getFeatureCount(waffle.page);
		expect(featureCount).toBeGreaterThanOrEqual(2);
	});

	test('exportStl returns result for model with mesh', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		try { await waitForMeshWithGeometry(waffle.page, 10000); } catch {
			await waffle.dumpState('export-stl-mesh-wait-failed');
		}

		const result = await waffle.page.evaluate(() => window.__waffle.exportStl());
		expect(result).toBeTruthy();

		// Canvas should still be visible (app didn't crash)
		await expect(waffle.page.locator('canvas')).toBeVisible();
	});

	// QUARANTINED (STEP-EXPORT capability gap): kernel-v2's `export_step` is the
	// trait-default NotSupported (root CLAUDE.md "Known capability boundaries";
	// docs/FILE_FORMAT.md §14.1), so the bridge rejects with "operation not
	// supported: export_step" — verified failing on a clean tree 2026-08-28,
	// independent of any local change. The loud-error contract is pinned by
	// `step_export_reports_the_kernel_capability_gap_loudly` in
	// crates/file-format/tests/format_tests.rs. Un-fixme (grep STEP-EXPORT)
	// when the kernel implements STEP export, and assert real STEP output then.
	test.fixme('exportStep returns result for model with mesh', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		try { await waitForMeshWithGeometry(waffle.page, 10000); } catch {
			await waffle.dumpState('export-step-mesh-wait-failed');
		}

		const result = await waffle.page.evaluate(() => window.__waffle.exportStep());
		expect(result).toBeTruthy();

		// Canvas should still be visible
		await expect(waffle.page.locator('canvas')).toBeVisible();
	});

	test('export with no model handles gracefully', async ({ waffle }) => {
		// Fresh state — no model created. Export throws when no mesh data exists.
		const stlResult = await waffle.page.evaluate(async () => {
			try {
				return await window.__waffle.exportStl();
			} catch {
				return null;
			}
		});
		expect(stlResult).toBeFalsy();

		const stepResult = await waffle.page.evaluate(async () => {
			try {
				return await window.__waffle.exportStep();
			} catch {
				return null;
			}
		});
		expect(stepResult).toBeFalsy();

		// Canvas should still be visible (app didn't crash)
		await expect(waffle.page.locator('canvas')).toBeVisible();
	});
});
