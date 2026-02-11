/**
 * Performance measurement helpers for GUI tests.
 *
 * Measures FPS, pick latency, and rebuild time using the __waffle test API
 * and browser requestAnimationFrame counters.
 */
import { expect } from '@playwright/test';

/**
 * Measure frames-per-second by injecting a rAF counter into the page.
 * Samples frame counts in 500ms buckets to derive a minimum FPS.
 *
 * @param {import('@playwright/test').Page} page
 * @param {number} [durationMs=2000] - measurement duration in milliseconds
 * @returns {Promise<{avg: number, min: number, frames: number, duration: number}>}
 */
export async function measureFPS(page, durationMs = 2000) {
	const result = await page.evaluate(async (duration) => {
		return new Promise((resolve) => {
			const bucketSize = 500;
			const buckets = [];
			let currentBucketFrames = 0;
			let totalFrames = 0;
			let startTime = null;
			let bucketStart = null;
			let rafId;

			function onFrame(timestamp) {
				if (startTime === null) {
					startTime = timestamp;
					bucketStart = timestamp;
				}

				const elapsed = timestamp - startTime;

				totalFrames++;
				currentBucketFrames++;

				// Check if current bucket is complete
				const bucketElapsed = timestamp - bucketStart;
				if (bucketElapsed >= bucketSize) {
					const bucketFps = (currentBucketFrames / bucketElapsed) * 1000;
					buckets.push(bucketFps);
					currentBucketFrames = 0;
					bucketStart = timestamp;
				}

				if (elapsed >= duration) {
					cancelAnimationFrame(rafId);
					const actualDuration = timestamp - startTime;
					const avg = (totalFrames / actualDuration) * 1000;
					const min = buckets.length > 0
						? Math.min(...buckets)
						: avg;
					resolve({
						avg: Math.round(avg * 100) / 100,
						min: Math.round(min * 100) / 100,
						frames: totalFrames,
						duration: Math.round(actualDuration),
					});
				} else {
					rafId = requestAnimationFrame(onFrame);
				}
			}

			rafId = requestAnimationFrame(onFrame);
		});
	}, durationMs);

	return result;
}

/**
 * Measure pick latency — time from a mouse click until __waffle.getHoveredRef()
 * changes (or a timeout is reached).
 *
 * @param {import('@playwright/test').Page} page
 * @param {number} x - absolute page x coordinate to click
 * @param {number} y - absolute page y coordinate to click
 * @param {number} [timeoutMs=3000] - max wait time in milliseconds
 * @returns {Promise<number>} latency in milliseconds
 */
export async function measurePickLatency(page, x, y, timeoutMs = 3000) {
	// Record the current hovered ref before clicking
	const beforeRef = await page.evaluate(() => {
		return window.__waffle?.getHoveredRef?.() ?? null;
	});

	const startTime = Date.now();

	// Click at the target position
	await page.mouse.click(x, y);

	// Wait for hovered ref to change or timeout
	try {
		await page.waitForFunction(
			(prev) => {
				const current = window.__waffle?.getHoveredRef?.() ?? null;
				return current !== prev;
			},
			beforeRef,
			{ timeout: timeoutMs }
		);
	} catch {
		// Timeout — return the full duration as the latency
	}

	const endTime = Date.now();
	return endTime - startTime;
}

/**
 * Read the last rebuild time from the __waffle API.
 *
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<number>} rebuild time in milliseconds
 */
export async function measureRebuildTime(page) {
	const time = await page.evaluate(() => {
		return window.__waffle?.getRebuildTime?.() ?? 0;
	});
	return time;
}

/**
 * Assert that measured FPS meets a minimum threshold.
 *
 * @param {{avg: number, min: number, frames: number, duration: number}} result
 * @param {{min: number}} thresholds - minimum acceptable average FPS
 */
export function expectFPS(result, { min }) {
	expect(result.avg).toBeGreaterThanOrEqual(min);
}

/**
 * Assert that measured latency is within an acceptable p95 budget.
 *
 * @param {number} result - latency in milliseconds
 * @param {{p95: number}} thresholds - maximum acceptable latency
 */
export function expectLatency(result, { p95 }) {
	expect(result).toBeLessThanOrEqual(p95);
}
