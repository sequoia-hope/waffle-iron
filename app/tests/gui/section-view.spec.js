/**
 * Capped section-view tests.
 *
 * Verifies the Section toolbar toggle: it captures the currently-selected
 * plane/face as the section plane, clips the solid bodies (and renders a
 * stencil cap), supports flip, and restores the normal view exactly on exit.
 *
 * Follows the project GUI rules: no try/catch around expected-state waits,
 * real interactions for the toggle, and crash detection via
 * collectCrashErrors + expectNoAnyCrash.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

const FRONT_PLANE_ID = '00000000-0000-0000-0000-000000000001';

/** Create a simple box solid: sketch a rectangle, extrude it. */
async function createBox(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.waitForTimeout(150);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: -0.02, y: -0.02, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: 0.02, y: -0.02, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: 0.02, y: 0.02, construction: false });
		w.addSketchEntity({ type: 'Point', id: 4, x: -0.02, y: 0.02, construction: false });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	});
	await page.waitForTimeout(150);

	await page.evaluate(() => window.__waffle.finishSketch());
	await waitForFeatureCount(page, 1, 10000);
	await page.waitForTimeout(150);

	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(() => window.__waffle.applyExtrude(0.04, 0, false));
	await waitForFeatureCount(page, 2, 10000);
	await waitForMeshWithGeometry(page);
}

/** Select the Front datum plane via the public API. */
async function selectFrontPlane(page) {
	const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: FRONT_PLANE_ID } };
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(50);
}

test.describe('capped section view', () => {
	test('toggle on with a selected plane activates section + clips the model', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		await createBox(page);

		// Section is off initially.
		let section = await page.evaluate(() => window.__waffle.getSectionState());
		expect(section.active).toBe(false);

		// Select the Front datum plane, then toggle Section via the toolbar button.
		await selectFrontPlane(page);
		await page.locator('[data-testid="toolbar-btn-section"]').click();
		await page.waitForTimeout(200);

		// State must be active with a captured plane (verify tool state).
		section = await page.evaluate(() => window.__waffle.getSectionState());
		expect(section.active).toBe(true);
		expect(section.plane).not.toBeNull();
		expect(section.plane.normal).toHaveLength(3);
		expect(section.flipped).toBe(false);

		// The captured plane has a real (non-zero) normal — this is what the
		// CadModel material effect and the SectionCap consume to clip + cap.
		expect(section.plane.normal.some((c) => c !== 0)).toBe(true);

		// Verify clipping reached the live render graph: at least one body
		// material now carries a clipping plane (rendered result changed).
		const clippedCount = await page.evaluate(() => window.__waffle.countClippedMaterials());
		expect(clippedCount).toBeGreaterThan(0);

		expectNoAnyCrash(crashTracker);
	});

	test('toggle on with no suitable selection stays off and hints', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		await createBox(page);

		// Nothing selected → clear any selection first.
		await page.evaluate(() => window.__waffle.clearSelection());
		await page.waitForTimeout(50);

		await page.locator('[data-testid="toolbar-btn-section"]').click();
		await page.waitForTimeout(150);

		const section = await page.evaluate(() => window.__waffle.getSectionState());
		expect(section.active).toBe(false);

		// A hint toast was shown.
		const toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.some((t) => /plane|face/i.test(t.message))).toBe(true);

		expectNoAnyCrash(crashTracker);
	});

	test('flip toggles the kept half; clear restores the normal view exactly', async ({ waffle }) => {
		const page = waffle.page;
		const crashTracker = collectCrashErrors(page);

		await createBox(page);
		await selectFrontPlane(page);

		await page.locator('[data-testid="toolbar-btn-section"]').click();
		await page.waitForTimeout(150);

		let section = await page.evaluate(() => window.__waffle.getSectionState());
		expect(section.active).toBe(true);
		expect(section.flipped).toBe(false);

		// Flip — the Flip button only exists while active.
		await page.locator('[data-testid="toolbar-btn-section-flip"]').click();
		await page.waitForTimeout(100);
		section = await page.evaluate(() => window.__waffle.getSectionState());
		expect(section.active).toBe(true);
		expect(section.flipped).toBe(true);

		// Clear/exit — state restored exactly (no plane, not flipped, no offset).
		await page.locator('[data-testid="toolbar-btn-section-clear"]').click();
		await page.waitForTimeout(100);
		section = await page.evaluate(() => window.__waffle.getSectionState());
		expect(section.active).toBe(false);
		expect(section.plane).toBeNull();
		expect(section.flipped).toBe(false);
		expect(section.offset).toBe(0);

		// Clipping removed from body materials — normal view restored exactly.
		const clippedAfterClear = await page.evaluate(() => window.__waffle.countClippedMaterials());
		expect(clippedAfterClear).toBe(0);

		// Toggling off via the button (re-toggle) also stays off afterward.
		await selectFrontPlane(page);
		await page.locator('[data-testid="toolbar-btn-section"]').click();
		await page.waitForTimeout(100);
		await page.locator('[data-testid="toolbar-btn-section"]').click();
		await page.waitForTimeout(100);
		section = await page.evaluate(() => window.__waffle.getSectionState());
		expect(section.active).toBe(false);

		expectNoAnyCrash(crashTracker);
	});
});
