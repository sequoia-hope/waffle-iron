/**
 * Click-vs-drag time threshold.
 *
 * A quick press/release (< DRAG_MIN_DURATION_MS) must be treated as a
 * click-in-place even if the pointer jittered past DRAG_THRESHOLD_PX — so a fast
 * click that twitches a few pixels does NOT drop a tiny line/segment. An
 * intentional click-drag (held past the threshold) still finalizes normally.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { getCanvasBounds } from './helpers/canvas.js';

const lineCount = (page) =>
	page.evaluate(() => window.__waffle.getEntities().filter((e) => e.type === 'Line').length);
const pointCount = (page) =>
	page.evaluate(() => window.__waffle.getEntities().filter((e) => e.type === 'Point').length);
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

test.describe('click-vs-drag time threshold (line tool)', () => {
	test('a fast jitter-click drops the start point but no tiny segment', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');
		await setTool(page, 'line');

		const b = await getCanvasBounds(page);
		const ax = b.centerX - 40;
		const ay = b.centerY - 20;

		// Quick press with a jitter that EXCEEDS the 5px distance threshold, but
		// released immediately (well under 200ms).
		await page.mouse.move(ax, ay);
		await page.mouse.down();
		await page.mouse.move(ax + 9, ay + 6);
		await page.mouse.up();

		// No tiny line was created — this is the bug being fixed.
		expect(await lineCount(page)).toBe(0);
		// The first point WAS dropped (click-in-place → click-click mode).
		expect(await pointCount(page)).toBeGreaterThanOrEqual(1);

		// A normal second click far away completes exactly one line.
		await page.mouse.click(b.centerX + 60, b.centerY + 40);
		await expect.poll(async () => await lineCount(page), { timeout: 3000 }).toBe(1);
	});

	test('an intentional held click-drag still finalizes a line', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');
		await setTool(page, 'line');

		const b = await getCanvasBounds(page);
		const cx = b.centerX - 60;
		const cy = b.centerY - 30;

		// Press, drag far, HOLD past the time threshold, then release.
		await page.mouse.move(cx, cy);
		await page.mouse.down();
		await page.mouse.move(cx + 100, cy + 70, { steps: 8 });
		await page.waitForTimeout(260);
		await page.mouse.up();

		await expect.poll(async () => await lineCount(page), { timeout: 3000 }).toBe(1);
	});
});
