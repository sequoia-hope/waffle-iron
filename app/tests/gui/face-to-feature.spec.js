/**
 * Phase D — face→feature (Tier 1). Clicking a face on the model highlights the
 * feature that created it (its producing feature) in the feature tree.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle, clickAt } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

test('clicking a face highlights its producing feature', async ({ waffle }) => {
	const crashes = collectCrashErrors(waffle.page);

	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, 1, 10000);
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('20');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, 2, 10000);

	// The extrude is feature-item-1; before clicking, it isn't flagged as the
	// face source.
	const extrude = waffle.page.locator('[data-testid="feature-item-1"]');
	await expect(extrude).not.toHaveClass(/face-source/);

	// Click the box face at the canvas centre.
	await clickAt(waffle.page, 0, 0);

	// Sanity: a face ref was actually picked.
	const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs?.() ?? []);
	expect(refs.length, 'a face was selected').toBeGreaterThan(0);

	// Its producing feature (the extrude) is now highlighted with the badge.
	await expect(extrude).toHaveClass(/face-source/);
	await expect(extrude.locator('.face-source-badge')).toBeVisible();

	expectNoAnyCrash(crashes);
});
