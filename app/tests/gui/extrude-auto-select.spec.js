/**
 * Extrude region auto-select.
 *
 *  - Shipped default (auto-select OFF): the dialog opens with NO region and
 *    region picking already armed, so the first click on a region adds it.
 *  - Auto-select ON, sketch with several regions (concentric circles): the
 *    pre-selected region is a real engine region, and Apply extrudes it —
 *    previously the list showed a placeholder that Apply then rejected with
 *    "click a region to extrude".
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickCircle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle, drawCircle } from './helpers/canvas.js';
import { waitForEntityCount, getFeatureCount, waitForFeatureCount, hasMeshWithGeometry, waitForMeshWithGeometry } from './helpers/state.js';

async function setAutoSelect(page, on) {
	await page.evaluate((v) => window.__waffle.updateSettings({ extrudeAutoSelectRegion: v }), on);
}

test.describe('extrude auto-select', () => {
	test('off (default): dialog opens empty with region picking armed', async ({ waffle }) => {
		const page = waffle.page;
		await setAutoSelect(page, false);
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);

		await clickExtrude(page);
		const regions = await page.evaluate(() => window.__waffle.getExtrudeRegions());
		expect(regions).toHaveLength(0);
		await expect(page.locator('[data-testid="extrude-region-0"]')).not.toBeVisible();
		const pick = await page.evaluate(() => window.__waffle.getProfilePickMode());
		expect(pick?.target).toBe('extrude');
		await expect(page.locator('[data-testid="extrude-region-box"]')).toHaveClass(/active/);
		await expect(page.locator('[data-testid="extrude-region-box"]')).toContainText('Click sketch profiles');
	});

	test('on: a multi-region sketch pre-selects a real region that Apply accepts', async ({ waffle }) => {
		const page = waffle.page;
		await setAutoSelect(page, true);
		await clickSketch(page);
		await clickCircle(page);
		await drawCircle(page, 0, 0, 110, 0);
		await waitForEntityCount(page, 2, 5000);
		await drawCircle(page, 0, 0, 45, 0);
		await waitForEntityCount(page, 3, 5000);
		await clickFinishSketch(page);
		const featuresBefore = await getFeatureCount(page);

		await clickExtrude(page);
		await expect(page.locator('[data-testid="extrude-region-0"]')).toBeVisible({ timeout: 5000 });
		const regions = await page.evaluate(() => window.__waffle.getExtrudeRegions());
		expect(regions).toHaveLength(1);
		// The pre-selected entry carries the engine's region geometry (what
		// Apply extrudes), not a bare profile placeholder.
		expect(regions[0].region).toBeTruthy();
		expect(Array.isArray(regions[0].region.outer)).toBe(true);
		const avail = await page.evaluate((id) => window.__waffle.getSketchRegions(id), regions[0].sketchId);
		expect(avail.length).toBeGreaterThanOrEqual(2);

		await page.locator('[data-testid="extrude-depth"]').fill('10');
		await page.locator('[data-testid="extrude-apply"]').click();
		await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
		await waitForFeatureCount(page, featuresBefore + 1, 10000);
		await waitForMeshWithGeometry(page, 10000);
		expect(await hasMeshWithGeometry(page)).toBe(true);
	});
});
