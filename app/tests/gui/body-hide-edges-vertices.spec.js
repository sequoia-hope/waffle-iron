/**
 * Hiding a body hides its edges and vertices.
 *
 * Regression for the request: "when I hide a body we need to hide its edges
 * and vertices." Previously CadModel gated faces on body visibility but
 * EdgeOverlay/VertexOverlay always rendered every body's edges/vertices.
 *
 * Oracle: the overlays publish their REAL rendered-array lengths to
 * window.__waffle.getRenderedOverlayCounts() — so we assert against the
 * actual rendered output, not a re-implemented filter.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount } from './helpers/state.js';

async function buildBox(waffle) {
	const page = waffle.page;
	await clickSketch(page);
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);

	await clickExtrude(page);
	const depthInput = page.locator('[data-testid="extrude-depth"]');
	if (await depthInput.isVisible()) await depthInput.fill('20');
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);
}

const counts = (page) => page.evaluate(() => window.__waffle.getRenderedOverlayCounts());

test.describe('body visibility hides edges and vertices', () => {
	test('hiding the only body drops edge and vertex render counts to zero', async ({ waffle }) => {
		const page = waffle.page;
		await buildBox(waffle);

		// A solid box renders edges and 8 deduplicated topological vertices.
		await expect.poll(async () => (await counts(page)).edgeBodies, { timeout: 8000 }).toBeGreaterThanOrEqual(1);
		await expect.poll(async () => (await counts(page)).vertices).toBeGreaterThanOrEqual(8);

		// Hide the body via the feature-tree visibility toggle.
		await page.locator('[data-testid="body-visibility-0"]').click();

		await expect.poll(async () => (await counts(page)).edgeBodies, { timeout: 8000 }).toBe(0);
		await expect.poll(async () => (await counts(page)).vertices).toBe(0);

		// Show it again — edges and vertices come back.
		await page.locator('[data-testid="body-visibility-0"]').click();

		await expect.poll(async () => (await counts(page)).edgeBodies, { timeout: 8000 }).toBeGreaterThanOrEqual(1);
		await expect.poll(async () => (await counts(page)).vertices).toBeGreaterThanOrEqual(8);
	});
});
