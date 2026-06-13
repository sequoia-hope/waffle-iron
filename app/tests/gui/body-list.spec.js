/**
 * Bodies-list tests — verifies the Bodies section under the feature tree:
 * listing solid bodies, selecting a body (whole-body highlight), and renaming.
 *
 * A "body" is the mesh produced by a renderable feature; its name is the
 * producing feature's name, so renaming a body renames that feature.
 *
 * Uses real DOM clicks. No assertion-swallowing — waits throw on timeout.
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
	getFeatureTree,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

const BODY_ITEM = '.body-item';

/** Create a sketch + extrude, producing exactly one solid body. */
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

test.describe('bodies list', () => {
	test('Bodies section lists one body after an extrude', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);
		await createBody(waffle);

		const section = waffle.page.locator('[data-testid="bodies-toggle"]');
		await expect(section).toBeVisible();
		await expect(section).toContainText('Bodies (1)');

		await expect(waffle.page.locator(BODY_ITEM)).toHaveCount(1);
		expectNoAnyCrash(crashes);
	});

	test('clicking a body highlights it (selected class), re-click clears', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);
		await createBody(waffle);

		const body = waffle.page.locator('[data-testid="body-item-0"]');
		await body.click();
		await expect(body).toHaveClass(/selected/);

		// Clicking the selected body again toggles it off.
		await body.click();
		await expect(body).not.toHaveClass(/selected/);
		expectNoAnyCrash(crashes);
	});

	test('double-click renames the body and its producing feature', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);
		await createBody(waffle);

		const body = waffle.page.locator('[data-testid="body-item-0"]');
		await body.dblclick();

		const input = body.locator('.body-rename-input');
		await expect(input).toBeVisible();
		await input.fill('Main Body');
		await waffle.page.keyboard.press('Enter');

		await expect(waffle.page.locator('[data-testid="body-item-0"]')).toContainText('Main Body');

		// The body name is the producing feature's name, so the feature renamed too.
		const tree = await getFeatureTree(waffle.page);
		const names = tree.features.map((f) => f.name);
		expect(names).toContain('Main Body');
		expectNoAnyCrash(crashes);
	});
});
