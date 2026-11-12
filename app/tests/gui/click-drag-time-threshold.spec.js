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
import { DRAG_THRESHOLD_PX, DRAG_MIN_DURATION_MS, DRAG_COMMIT_PX } from '../../src/lib/config.js';

const lineCount = (page) =>
	page.evaluate(() => window.__waffle.getEntities().filter((e) => e.type === 'Line').length);
const pointCount = (page) =>
	page.evaluate(() => window.__waffle.getEntities().filter((e) => e.type === 'Point').length);
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

/**
 * Put the page's `Date.now()` under the test's control: it stands still until
 * `advanceAppClock` moves it. The pointer events stay real; only the clock the
 * app times the press with (`pointerDownTime` in tools.js) is the test's.
 *
 * A wall-clock "quick press" cannot be produced through Playwright under load.
 * On Chromium a `mouse.move` with the left button down is not one event: the
 * drag manager evaluates a listener into every frame, toggles
 * `Input.setInterceptDrags`, moves, then evaluates again to ask whether a drag
 * started. Measured with the suite running in parallel, that put 100–680 ms
 * between pointerdown and pointerup — at or past DRAG_MIN_DURATION_MS, so the
 * app correctly read a HELD drag and finalized the segment. The test was
 * asserting the click branch on a gesture that had become a hold.
 */
async function holdAppClock(page) {
	await page.evaluate(() => {
		const real = Date.now;
		let t = real.call(Date);
		Date.now = () => t;
		window.__testClock = {
			advance: (ms) => { t += ms; },
			release: () => { Date.now = real; delete window.__testClock; },
		};
	});
}
const advanceAppClock = (page, ms) => page.evaluate((d) => window.__testClock.advance(d), ms);
const releaseAppClock = (page) => page.evaluate(() => window.__testClock.release());

test.describe('click-vs-drag time threshold (line tool)', () => {
	test('a fast jitter-click drops the start point but no tiny segment', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');
		await setTool(page, 'line');

		const b = await getCanvasBounds(page);
		const ax = b.centerX - 40;
		const ay = b.centerY - 20;

		// Quick press with a jitter that EXCEEDS the drag distance threshold
		// (so the press IS flagged as a drag) but stays under the unambiguous
		// commit distance, released well under the hold time: only the time
		// gate stands between this gesture and a tiny segment.
		const JITTER = { x: 9, y: 6 };
		const PRESS_MS = 80;
		const jitterPx = Math.hypot(JITTER.x, JITTER.y);
		expect(jitterPx).toBeGreaterThan(DRAG_THRESHOLD_PX);
		expect(jitterPx).toBeLessThan(DRAG_COMMIT_PX);
		expect(PRESS_MS).toBeLessThan(DRAG_MIN_DURATION_MS);

		await page.mouse.move(ax, ay);
		await page.evaluate(() => window.__waffle.clearToolEventLog());
		await holdAppClock(page);
		await page.mouse.down();
		await page.mouse.move(ax + JITTER.x, ay + JITTER.y);
		await advanceAppClock(page, PRESS_MS);
		await page.mouse.up();
		await releaseAppClock(page);

		// The gesture the app saw is the one described: the release arrived
		// with the drag flag already set by the jitter, PRESS_MS after the press.
		const log = await page.evaluate(() => window.__waffle.getToolEventLog());
		const down = log.find((e) => e.event === 'pointerdown');
		const up = log.find((e) => e.event === 'pointerup');
		expect(down, 'pointerdown reached the line tool').toBeTruthy();
		expect(up, 'pointerup reached the line tool').toBeTruthy();
		expect(up.isDragging, 'the jitter crossed the drag distance threshold').toBe(true);
		expect(up.timestamp - down.timestamp).toBe(PRESS_MS);

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
