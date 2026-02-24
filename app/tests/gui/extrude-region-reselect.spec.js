/**
 * Extrude region re-selection tests — verifies that the extrude dialog
 * allows replacing the selected region, not just appending.
 *
 * Bug: After opening extrude dialog (auto-populated with sketch profile),
 * the user cannot replace the selected region. "Click to pick" only appends.
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
	getExtrudeDialogState,
} from './helpers/state.js';

test.describe('extrude region re-selection', () => {
	test('extrude dialog auto-populates with 1 region', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		try { await waitForEntityCount(page, 8, 3000); } catch {
			await waffle.dumpState('region-draw');
		}
		await clickFinishSketch(page);
		try { await waitForFeatureCount(page, 1, 10000); } catch {
			await waffle.dumpState('region-finish');
		}

		await clickExtrude(page);

		// Verify 1 region is auto-populated
		const state = await getExtrudeDialogState(page);
		expect(state).toBeTruthy();
		expect(state.regions.length).toBe(1);
	});

	test('remove region via X button clears to empty state', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		try { await waitForEntityCount(page, 8, 3000); } catch {
			await waffle.dumpState('region-rm-draw');
		}
		await clickFinishSketch(page);
		try { await waitForFeatureCount(page, 1, 10000); } catch {
			await waffle.dumpState('region-rm-finish');
		}

		await clickExtrude(page);

		// Verify 1 region exists
		let state = await getExtrudeDialogState(page);
		expect(state.regions.length).toBe(1);

		// Click the remove button on the first region
		await page.locator('[data-testid="extrude-region-0"] .region-remove').click();
		await page.waitForTimeout(200);

		// Verify regions are now empty
		state = await getExtrudeDialogState(page);
		expect(state.regions.length).toBe(0);
	});

	test('toggling region pick mode clears existing regions', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		try { await waitForEntityCount(page, 8, 3000); } catch {
			await waffle.dumpState('region-pick-draw');
		}
		await clickFinishSketch(page);
		try { await waitForFeatureCount(page, 1, 10000); } catch {
			await waffle.dumpState('region-pick-finish');
		}

		await clickExtrude(page);

		// Verify 1 region auto-populated
		let state = await getExtrudeDialogState(page);
		expect(state.regions.length).toBe(1);

		// Click the region box to toggle pick mode ON
		await page.locator('[data-testid="extrude-region-box"]').click();
		await page.waitForTimeout(200);

		// When pick mode is toggled ON, existing regions should be cleared
		state = await getExtrudeDialogState(page);
		expect(state.regions.length).toBe(0);

		// Verify pick mode is active
		const pickActive = await page.evaluate(
			() => window.__waffle?.getExtrudeRegionPickMode?.() === true
		);
		expect(pickActive).toBe(true);
	});

	test('region pick mode hint text changes', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		try { await waitForEntityCount(page, 8, 3000); } catch {
			await waffle.dumpState('region-hint-draw');
		}
		await clickFinishSketch(page);
		try { await waitForFeatureCount(page, 1, 10000); } catch {
			await waffle.dumpState('region-hint-finish');
		}

		await clickExtrude(page);

		// Initially should show "Click to pick"
		const hintBefore = await page.locator('.pick-hint').textContent();
		expect(hintBefore.trim()).toBe('Click to pick');

		// Click region box to toggle pick mode
		await page.locator('[data-testid="extrude-region-box"]').click();
		await page.waitForTimeout(200);

		// Should now show "Click faces to add..."
		const hintAfter = await page.locator('.pick-hint').textContent();
		expect(hintAfter.trim()).toBe('Click faces to add...');
	});
});
