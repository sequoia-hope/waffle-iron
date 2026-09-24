/**
 * The "display rotation center" debug marker (Settings → Debug).
 *
 * A translucent green sphere at the point an orbit turns about — shown ONLY
 * while a rotate is in progress, and only when the (sticky) setting is on.
 * What it marks is `controls.orbitPivot`: the model point under the cursor at
 * the start of the rotate, which is a different thing from the look-at target
 * and is exactly what is worth being able to see.
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds } from './helpers/canvas.js';
import { createExtrudedBox } from './helpers/geometry.js';

const marker = (page) => page.evaluate(() => window.__waffle.getRotationCenter());

/** Press, move a little, and read the marker mid-drag; then release. */
async function duringRotate(page, fn) {
	const b = await getCanvasBounds(page);
	await page.mouse.move(b.centerX, b.centerY);
	await page.mouse.down();
	await page.mouse.move(b.centerX + 30, b.centerY + 20, { steps: 5 });
	await page.waitForTimeout(150);
	const out = await fn();
	await page.mouse.up();
	await page.waitForTimeout(150);
	return out;
}

test.describe('rotation center marker', () => {
	test('the Debug section offers it, and the checkbox drives the setting', async ({ waffle }) => {
		const page = waffle.page;
		await page.locator('[data-testid="toolbar-btn-settings"]').click();
		await page.locator('[data-testid="settings-section-debug"]').click();
		const box = page.locator('[data-testid="setting-show-rotation-center"]');
		await expect(box).toBeVisible();
		await expect(box).not.toBeChecked();
		await box.check();
		expect((await marker(page)).enabled, 'the checkbox writes the setting').toBe(true);
	});

	test('off by default, and off between rotates when on', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(300);

		expect((await marker(page)).enabled, 'off by default').toBe(false);
		const during = await duringRotate(page, () => marker(page));
		expect(during.visible, 'nothing to see while the setting is off').toBe(false);

		await page.evaluate(() => window.__waffle.updateSettings({ showRotationCenter: true }));
		expect((await marker(page)).enabled).toBe(true);
		expect((await marker(page)).visible, 'not while nothing is rotating').toBe(false);
	});

	test('while rotating, it marks the point the view turns about', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(300);
		await page.evaluate(() => window.__waffle.updateSettings({ showRotationCenter: true }));

		const during = await duringRotate(page, () => marker(page));
		expect(during.visible, 'visible mid-rotate').toBe(true);
		expect(during.at, 'and it has a point to mark').not.toBeNull();
		// The drag started over the part, so the pivot is ON it.
		const box = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
		for (let i = 0; i < 3; i++) {
			expect(during.at[i]).toBeGreaterThanOrEqual(box.min[i] - 1e-6);
			expect(during.at[i]).toBeLessThanOrEqual(box.max[i] + 1e-6);
		}

		// And it goes away when the gesture ends.
		expect((await marker(page)).visible, 'hidden again after the rotate').toBe(false);
	});

	test('the setting is sticky across a reload', async ({ waffle }) => {
		const page = waffle.page;
		await page.evaluate(() => window.__waffle.updateSettings({ showRotationCenter: true }));
		await page.reload();
		await page.waitForFunction(() => window.__waffle?.getRotationCenter);
		expect((await marker(page)).enabled, 'remembered').toBe(true);
	});
});
