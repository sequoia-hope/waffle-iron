/**
 * Zooming in past the sketch extents.
 *
 * Bug: after drawing a small box and a big box, you could not zoom into the
 * small box — the auto-fit effect snapped the zoom back out to keep the whole
 * sketch (the big box) on screen. Normal zoom should be allowed to go past the
 * sketch entities and let them flow off screen.
 *
 * Oracle: in sketch mode the camera is orthographic, so the visible half-height
 * is getCameraState().frustumTop. After zooming in, 2*frustumTop (the visible
 * height) must be able to drop BELOW the big box's extent.
 */
import { test, expect } from './helpers/waffle-test.js';
import { drawRectangle, zoom } from './helpers/canvas.js';
import { clickSketch } from './helpers/toolbar.js';

const positions = (page) => page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const cam = (page) => page.evaluate(() => window.__waffle.getCameraState());
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

async function drawRect(page, x1, y1, x2, y2) {
	// Re-arm via select so a fresh rectangle starts cleanly.
	await setTool(page, 'select');
	await setTool(page, 'rectangle');
	await drawRectangle(page, x1, y1, x2, y2);
}

function bigExtent(pos) {
	let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
	for (const p of Object.values(pos)) {
		minX = Math.min(minX, p.x); minY = Math.min(minY, p.y);
		maxX = Math.max(maxX, p.x); maxY = Math.max(maxY, p.y);
	}
	return Math.max(maxX - minX, maxY - minY);
}

test.describe('sketch zoom past extents', () => {
	test('can zoom in until the big box flows off screen', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');

		// A large box spanning most of the view, then a small box near center.
		await drawRect(page, -180, -130, 180, 130);
		await drawRect(page, 15, 15, 45, 45);
		await page.waitForTimeout(400); // let the (one-time) auto-fit settle

		const ext = bigExtent(await positions(page));
		expect(ext).toBeGreaterThan(0);

		const before = await cam(page);
		expect(before.projection).toBe('orthographic');
		// After the auto-fit the whole sketch fits: visible height >= big extent.
		expect(2 * before.frustumTop).toBeGreaterThan(ext * 0.9);

		// Zoom in repeatedly toward the center.
		for (let i = 0; i < 6; i++) await zoom(page, -300);
		await page.waitForTimeout(300);

		const after = await cam(page);
		expect(Number.isFinite(after.frustumTop)).toBe(true);
		expect(after.frustumTop).toBeGreaterThan(0);

		// The visible height is now well below the big box extent — i.e. the big
		// box has flowed off screen. Pre-fix the auto-fit clamped it back to ~ext.
		expect(2 * after.frustumTop).toBeLessThan(ext * 0.8);
	});
});
