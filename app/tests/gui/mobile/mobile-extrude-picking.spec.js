/**
 * Mobile extrude picking — iPhone bug report 2026-11-03.
 *
 * On touch there are no hover moves between taps, so:
 *  - region picking (a WINDOW pointerdown listener reading the stored hover)
 *    re-added the last-tapped region on every tap of the extrude dialog —
 *    its fields, even the region's ✕;
 *  - the first tap on a sketch region found no hover and fell through to the
 *    datum plane under it, selecting the plane instead;
 *  - a selected plane could not be deselected in a sketch-only document
 *    (the background-miss handler lived on body meshes only);
 *  - the extrude bottom sheet covered half the framed model.
 *
 * Runs in the mobile-portrait and mobile-landscape projects; taps are real
 * touch events (page.touchscreen / locator.tap).
 */
import { test, expect } from '../helpers/waffle-test.js';
import { clickSketch, clickCircle, clickFinishSketch, clickExtrude } from '../helpers/toolbar.js';
import { drawCircle, getCanvasBounds } from '../helpers/canvas.js';
import { waitForFeatureCount, getExtrudeDialogState } from '../helpers/state.js';

const regionCount = async (page) => (await getExtrudeDialogState(page))?.regions?.length ?? -1;
const selectedRefs = (page) => page.evaluate(() => window.__waffle.getSelectedRefs());

async function tapWorld(page, x, y, z) {
	const p = await page.evaluate(([x, y, z]) => window.__waffle.worldPointToScreen(x, y, z), [x, y, z]);
	expect(p).toBeTruthy();
	await page.touchscreen.tap(p.x, p.y);
	await page.waitForTimeout(250);
}

/** Sketch a circle around the origin, finish, open Extrude. */
async function circleSketchIntoExtrude(page) {
	await clickSketch(page);
	await clickCircle(page);
	await drawCircle(page, 0, 0, 70, 0);
	await page.waitForFunction(
		() => (window.__waffle.getEntities?.() ?? window.__waffle.getState().sketchEntities ?? [])
			.some((e) => e.type === 'Circle'),
		null,
		{ timeout: 5000 }
	);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
}

test.describe('mobile extrude picking', () => {
	test('dialog taps never re-add a region; a re-tapped region does not stack', async ({ waffle }) => {
		const page = waffle.page;
		await circleSketchIntoExtrude(page);
		await expect.poll(() => regionCount(page)).toBe(1); // auto-populated

		// ✕ removes it, and stays removed.
		await page.locator('[data-testid="extrude-region-0"] .region-remove').tap();
		await page.waitForTimeout(250);
		expect(await regionCount(page)).toBe(0);

		// Tap the circle (the Front datum plane lies right under it): the
		// region is picked on the FIRST tap and the plane is not selected.
		await tapWorld(page, 0, 0, 0);
		expect(await regionCount(page)).toBe(1);
		expect((await selectedRefs(page)).some((r) => r?.anchor?.type === 'DatumPlane')).toBe(false);

		// Re-tapping the same circle does not stack a duplicate.
		await tapWorld(page, 0, 0, 0);
		expect(await regionCount(page)).toBe(1);

		// Tapping around the dialog must not re-add the last-tapped region.
		const dialog = page.locator('[data-testid="extrude-dialog"]');
		const field = dialog.locator('input').first();
		await field.tap();
		await page.locator('[data-testid="extrude-dialog"] .dialog-header').tap();
		await field.tap();
		await page.waitForTimeout(250);
		expect(await regionCount(page)).toBe(1);

		await page.locator('[data-testid="extrude-region-0"] .region-remove').tap();
		await page.waitForTimeout(250);
		await page.locator('[data-testid="extrude-dialog"] .dialog-header').tap();
		await page.waitForTimeout(250);
		expect(await regionCount(page)).toBe(0);
	});

	test('the 3D view ends at the edge of the extrude dialog', async ({ waffle }) => {
		const page = waffle.page;
		await circleSketchIntoExtrude(page);
		const dialog = page.locator('[data-testid="extrude-dialog"]');
		const vp = await page.locator('[data-testid="viewport"]').boundingBox();
		await expect.poll(async () => {
			const d = await dialog.boundingBox();
			const c = await getCanvasBounds(page);
			const bottomSheet = d.width >= 0.8 * vp.width;
			// Bottom sheet: the canvas stops at its top edge. Side panel (phone
			// held sideways): the canvas stops at its left edge.
			return bottomSheet
				? c.y + c.height <= d.y + 1
				: c.x + c.width <= d.x + 1;
		}).toBe(true);
		// The circle the user framed is still in the visible canvas.
		const p = await page.evaluate(() => window.__waffle.worldPointToScreen(0, 0, 0));
		const c = await getCanvasBounds(page);
		expect(p.x).toBeGreaterThan(c.x);
		expect(p.x).toBeLessThan(c.x + c.width);
		expect(p.y).toBeGreaterThan(c.y);
		expect(p.y).toBeLessThan(c.y + c.height);
	});

	test('a selected plane deselects by tapping it again or tapping empty space', async ({ waffle }) => {
		const page = waffle.page;
		const isPlane = (refs) => refs.length === 1 && refs[0]?.anchor?.type === 'DatumPlane';

		await tapWorld(page, 0.01, 0.01, 0);
		expect(isPlane(await selectedRefs(page))).toBe(true);
		await tapWorld(page, 0.01, 0.01, 0);
		expect(await selectedRefs(page)).toEqual([]);

		await tapWorld(page, 0.01, 0.01, 0);
		expect(isPlane(await selectedRefs(page))).toBe(true);
		// Far outside every 200 mm datum plane, still inside the canvas.
		const c = await getCanvasBounds(page);
		await page.touchscreen.tap(c.x + 8, c.y + c.height - 8);
		await page.waitForTimeout(250);
		expect(await selectedRefs(page)).toEqual([]);
	});
});
