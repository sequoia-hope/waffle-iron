/**
 * Cancel and Recovery tests — verifies clean cancellation and undo/redo recovery.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	pressKey,
} from './helpers/toolbar.js';
import { clickAt, drawRectangle } from './helpers/canvas.js';
import {
	isSketchActive,
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	waitForMeshWithGeometry,
} from './helpers/state.js';

const FEATURE_ITEM = '.tree-item:not(.origin-item)';

async function createSketchAndExtrude(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
		await waffle.dumpState('cr-sketch-draw-failed');
	}
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
		await waffle.dumpState('cr-sketch-finish-failed');
	}
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
		await waffle.dumpState('cr-extrude-failed');
	}
}

test.describe('cancel and recovery', () => {
	test('cancel sketch mid-drawing resets cleanly', async ({ waffle }) => {
		// Enter sketch mode and select rectangle tool
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);

		// Click once (first corner only — incomplete rectangle)
		await clickAt(waffle.page, -80, -60);

		// Press Escape to cancel drawing
		await pressKey(waffle.page, 'Escape');
		await waffle.page.waitForTimeout(200);

		// Press Escape again to exit sketch mode if still active
		const stillActive = await isSketchActive(waffle.page);
		if (stillActive) {
			await pressKey(waffle.page, 'Escape');
			await waffle.page.waitForTimeout(500);
		}

		// Verify sketch mode is deactivated
		const active = await isSketchActive(waffle.page);
		expect(active).toBe(false);

		// Canvas should be visible
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('cancel extrude dialog then retry succeeds', async ({ waffle }) => {
		// Create and finish a sketch
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		try { await waitForEntityCount(waffle.page, 8, 3000); } catch {
			await waffle.dumpState('cr-cancel-extrude-draw-failed');
		}
		await clickFinishSketch(waffle.page);
		try { await waitForFeatureCount(waffle.page, 1, 10000); } catch {
			await waffle.dumpState('cr-cancel-extrude-finish-failed');
		}

		// Click Extrude, then Cancel
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-cancel"]').click();

		// Verify dialog is not visible
		await expect(waffle.page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();

		// Click Extrude again, fill depth, Apply
		await clickExtrude(waffle.page);
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Verify 2 features (Sketch + Extrude)
		try { await waitForFeatureCount(waffle.page, 2, 10000); } catch {
			await waffle.dumpState('cr-retry-extrude-failed');
		}
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(2);
	});

	test('undo after extrude removes it', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Verify we start with 2 features
		expect(await getFeatureCount(waffle.page)).toBe(2);

		// Press Ctrl+Z to undo
		await waffle.page.keyboard.press('Control+z');
		await waffle.page.waitForTimeout(500);

		// Verify feature count is 1 (only sketch remains)
		const count = await getFeatureCount(waffle.page);
		expect(count).toBe(1);

		// Verify Extrude is gone
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(false);
	});

	test('redo after undo restores extrude', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Ctrl+Z to undo extrude
		await waffle.page.keyboard.press('Control+z');
		await waffle.page.waitForTimeout(500);

		// Verify feature count is 1
		expect(await getFeatureCount(waffle.page)).toBe(1);

		// Ctrl+Shift+Z to redo
		await waffle.page.keyboard.press('Control+Shift+z');
		await waffle.page.waitForTimeout(500);

		// Wait for feature count to reach 2
		try { await waitForFeatureCount(waffle.page, 2, 5000); } catch {
			await waffle.dumpState('cr-redo-failed');
		}

		// Verify Extrude is restored
		expect(await getFeatureCount(waffle.page)).toBe(2);
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);
	});

	test('abandon empty sketch-on-face preserves original model', async ({ waffle }) => {
		await createSketchAndExtrude(waffle);

		// Wait for mesh to appear
		try { await waitForMeshWithGeometry(waffle.page, 10000); } catch {
			await waffle.dumpState('cr-abandon-mesh-wait-failed');
		}

		// Verify mesh exists before
		const hasMeshBefore = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshBefore).toBe(true);

		// Get a face ref from the mesh
		const ref = await waffle.page.evaluate(() => {
			const m = window.__waffle.getMeshes().find(m => m.faceRangeCount > 0);
			return m?.faceRanges?.[0]?.geom_ref ?? null;
		});

		if (ref) {
			// Select the face
			await waffle.page.evaluate(r => window.__waffle.selectRef(r), ref);
			await waffle.page.waitForTimeout(200);

			// Click Sketch button (enters sketch on face)
			const sketchBtn = waffle.page.locator('[data-testid="toolbar-btn-sketch"]');
			await sketchBtn.click();

			// Wait for sketch mode to activate
			try {
				await waffle.page.waitForFunction(
					() => window.__waffle?.getState()?.sketchMode?.active === true,
					{ timeout: 5000 }
				);
			} catch {
				await waffle.dumpState('cr-abandon-sketch-on-face-enter-failed');
			}

			// Immediately finish (empty sketch)
			await clickFinishSketch(waffle.page);
		} else {
			// Fallback: enter a normal sketch and immediately finish
			await clickSketch(waffle.page);
			await clickFinishSketch(waffle.page);
		}

		// Verify sketch mode is deactivated
		const active = await isSketchActive(waffle.page);
		expect(active).toBe(false);

		// Verify original mesh is still intact
		const hasMeshAfter = await hasMeshWithGeometry(waffle.page);
		expect(hasMeshAfter).toBe(true);
	});
});
