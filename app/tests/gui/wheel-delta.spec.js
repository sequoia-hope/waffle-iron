/**
 * Wheel zoom honours `WheelEvent.deltaMode` (REVIEW.md §3.4, remediation Q4).
 *
 * Chromium reports wheel deltas in pixels (mode 0, ~100 per notch); Firefox
 * with a mouse wheel reports LINES (mode 1, 3 per notch), and paged scrolling
 * reports PAGES (mode 2). Read raw, a Firefox notch zoomed ~30x less than a
 * Chromium one. The normalizer tests are pure; the last test
 * checks the viewport's wheel handler uses it.
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds } from './helpers/canvas.js';
import { wheelDeltaPixels, WHEEL_LINE_PX } from '../../src/lib/viewport/wheel.js';

const PAGE = 628; // a viewport height, px

test('pixel mode passes through', () => {
	expect(wheelDeltaPixels({ deltaY: -100, deltaMode: 0 }, PAGE)).toBe(-100);
	expect(wheelDeltaPixels({ deltaY: 4.5, deltaMode: 0 }, PAGE)).toBe(4.5);
});

test('line mode scales by the line height', () => {
	expect(wheelDeltaPixels({ deltaY: 3, deltaMode: 1 }, PAGE)).toBe(3 * WHEEL_LINE_PX);
	expect(WHEEL_LINE_PX).toBeGreaterThanOrEqual(16);
	expect(WHEEL_LINE_PX).toBeLessThanOrEqual(40);
	// One Firefox notch (3 lines) lands within 2x of one Chromium notch (100 px).
	const ratio = wheelDeltaPixels({ deltaY: 3, deltaMode: 1 }, PAGE) / 100;
	expect(ratio).toBeGreaterThan(0.5);
	expect(ratio).toBeLessThan(2);
});

test('page mode scales by the viewport height', () => {
	expect(wheelDeltaPixels({ deltaY: -1, deltaMode: 2 }, PAGE)).toBe(-PAGE);
});

test('unknown mode and missing fields are treated as pixels / zero', () => {
	expect(wheelDeltaPixels({ deltaY: 7 }, PAGE)).toBe(7);
	expect(wheelDeltaPixels({ deltaY: 7, deltaMode: 9 }, PAGE)).toBe(7);
	expect(wheelDeltaPixels({ deltaMode: 1 }, PAGE)).toBe(0);
});

/** A scalar view size: orbit distance (perspective) or half-height (ortho). */
async function viewScale(page) {
	return page.evaluate(() => {
		const c = window.__waffle.getCameraState();
		if (c.projection === 'orthographic') return c.frustumTop / c.zoom;
		return Math.hypot(c.position[0] - c.target[0], c.position[1] - c.target[1], c.position[2] - c.target[2]);
	});
}

/** Dispatch one wheel event at the canvas centre. */
async function wheel(page, deltaY, deltaMode) {
	const b = await getCanvasBounds(page);
	await page.evaluate(([x, y, dy, mode]) => {
		const canvas = document.querySelector('[data-testid="viewport"] canvas');
		canvas.dispatchEvent(new WheelEvent('wheel', { clientX: x, clientY: y, deltaY: dy, deltaMode: mode, bubbles: true, cancelable: true }));
	}, [b.centerX, b.centerY, deltaY, deltaMode]);
	await page.waitForTimeout(100);
}

test('a line-mode notch zooms the viewport like a pixel-mode notch', async ({ waffle }) => {
	const page = waffle.page;
	await page.waitForFunction(() => window.__waffle?.getCameraState() != null);
	const s0 = await viewScale(page);
	await wheel(page, -3 * WHEEL_LINE_PX, 0);
	const s1 = await viewScale(page);
	const pixelRatio = s1 / s0;
	expect(pixelRatio, 'pixel-mode wheel zoomed in').toBeLessThan(0.99);

	await wheel(page, -3, 1);
	const s2 = await viewScale(page);
	const lineRatio = s2 / s1;
	expect(Math.abs(lineRatio - pixelRatio), `line ${lineRatio} vs pixel ${pixelRatio}`).toBeLessThan(1e-6);
});
