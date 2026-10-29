/**
 * Small features at mm units (REVIEW.md §3.2-3, remediation Q2).
 *
 * The circle tool silently dropped any radius ≤ 1 mm (a `radius > 0.001`
 * metre literal) and the slot tool reset when its centres were < 1 mm apart.
 * What counts as degenerate is a SCREEN question — a radius the user could
 * not have meant because it is a few pixels — so at a zoom where 0.4 mm is
 * dozens of pixels both tools must make the feature.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickCircle, pressKey } from './helpers/toolbar.js';
import { clickAt, dragCircle, moveTo, zoom } from './helpers/canvas.js';
import { waitForEntityCount, getEntities } from './helpers/state.js';

const R = 0.0004; // 0.4 mm, in metres

/** Zoom in until one screen pixel is at most `mPerPx` metres. */
async function zoomUntil(page, mPerPx) {
	for (let i = 0; i < 60; i++) {
		// zoom() parks the pointer at the canvas centre; a 1 px nudge makes the
		// sketch tools re-read the pixel size at the new zoom.
		await zoom(page, i === 0 ? 0 : -400);
		await moveTo(page, i % 2, 0);
		const px = await page.evaluate(() => window.__waffle.getSketchPixelSize());
		if (px > 0 && px <= mPerPx) return px;
	}
	const px = await page.evaluate(() => window.__waffle.getSketchPixelSize());
	expect(px, 'zoomed in far enough').toBeLessThanOrEqual(mPerPx);
	return px;
}

async function enterAndZoom(page) {
	await clickSketch(page);
	const unit = await page.evaluate(() => window.__waffle.getDocumentState().documentDisplayUnit);
	expect(unit).toBe('mm');
	// 0.4 mm ≥ 40 px.
	return zoomUntil(page, R / 40);
}

test.describe('sub-millimetre sketch features', () => {
	test('circle of 0.4 mm radius by click-click', async ({ waffle }) => {
		const page = waffle.page;
		const px = await enterAndZoom(page);
		await clickCircle(page);
		const rPx = R / px;
		await clickAt(page, -20, 10);
		await clickAt(page, -20 + rPx, 10);
		await waitForEntityCount(page, 2, 3000);
		const circles = (await getEntities(page)).filter((e) => e.type === 'Circle');
		expect(circles.length, 'a circle was created').toBe(1);
		expect(Math.abs(circles[0].radius - R) / R, `radius ${circles[0].radius}`).toBeLessThan(0.05);
	});

	test('circle of 0.4 mm radius by click-drag', async ({ waffle }) => {
		const page = waffle.page;
		const px = await enterAndZoom(page);
		await clickCircle(page);
		const rPx = R / px;
		await dragCircle(page, -20, 10, -20 + rPx, 10);
		await waitForEntityCount(page, 2, 3000);
		const circles = (await getEntities(page)).filter((e) => e.type === 'Circle');
		expect(circles.length, 'a circle was created').toBe(1);
		expect(Math.abs(circles[0].radius - R) / R, `radius ${circles[0].radius}`).toBeLessThan(0.05);
	});

	test('slot with centres 0.8 mm apart', async ({ waffle }) => {
		const page = waffle.page;
		const px = await enterAndZoom(page);
		await pressKey(page, 't');
		await page.waitForFunction(() => window.__waffle?.getState()?.activeTool === 'slot', null, { timeout: 3000 });
		const dPx = (2 * R) / px;
		await clickAt(page, -dPx / 2, 0);
		await clickAt(page, dPx / 2, 0);

		const popup = await page.evaluate(() => window.__waffle.getDimensionPopup());
		expect(popup, 'width popup opened').not.toBeNull();
		// The default width is a third of the centre distance — not a 0.5 m floor.
		expect(popup.defaultValue, `default width ${popup.defaultValue}`).toBeLessThan(2 * R);

		const input = page.locator('.dimension-input');
		await expect(input).toBeVisible({ timeout: 2000 });
		await input.fill('0.3'); // mm
		await page.keyboard.press('Enter');

		await expect.poll(async () => (await getEntities(page)).filter((e) => e.type === 'Arc').length,
			{ message: 'slot arcs created', timeout: 3000 }).toBe(2);
		const arcs = (await getEntities(page)).filter((e) => e.type === 'Arc');
		const pos = await page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
		const c0 = pos[arcs[0].center_id], c1 = pos[arcs[1].center_id];
		const d = Math.hypot(c1.x - c0.x, c1.y - c0.y);
		expect(Math.abs(d - 2 * R) / (2 * R), `centre distance ${d}`).toBeLessThan(0.05);
	});
});
