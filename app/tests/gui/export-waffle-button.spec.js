/** The main-toolbar "Export .waffle" button downloads a .waffle file. */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

test('Export .waffle on the main toolbar downloads a .waffle', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);

	const btn = waffle.page.locator('[data-testid="toolbar-btn-export-waffle-main"]');
	await expect(btn).toBeVisible();
	const dl = waffle.page.waitForEvent('download');
	await btn.click();
	const download = await dl;
	expect(download.suggestedFilename()).toMatch(/\.waffle$/);
	expectNoAnyCrash(crashes);
});
