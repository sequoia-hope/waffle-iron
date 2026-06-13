/**
 * Phase C — per-body STL export via the Bodies-list right-click menu.
 *
 * Right-clicking a body → "Export STL" downloads that body as `<name>.stl`.
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
	collectCrashErrors,
	expectNoAnyCrash,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

async function createBody(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);
}

test('right-click a body → Export STL downloads that body', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);
	await createBody(waffle);

	const body = waffle.page.locator('[data-testid="body-item-0"]');
	await body.click({ button: 'right' });

	const exportItem = waffle.page.locator('[data-testid="body-ctx-export-stl"]');
	await expect(exportItem).toBeVisible();

	const downloadPromise = waffle.page.waitForEvent('download');
	await exportItem.click();
	const download = await downloadPromise;
	expect(download.suggestedFilename()).toMatch(/\.stl$/);

	// Menu closes after export.
	await expect(exportItem).not.toBeVisible();
	expectNoAnyCrash(crashes);
});
