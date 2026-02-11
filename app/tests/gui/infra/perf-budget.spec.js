/**
 * Infrastructure tests — performance measurement helpers.
 *
 * Verifies that the perf helpers return reasonable values in a headless
 * environment (SwiftShader may produce low FPS — these tests only check
 * that the measurement pipeline works, not that performance is fast).
 */
import { test, expect } from '../helpers/waffle-test.js';
import { measureFPS, measurePickLatency, measureRebuildTime, expectFPS } from '../helpers/perf.js';
import { getCanvasBounds } from '../helpers/canvas.js';

test.describe('performance budget infrastructure', () => {
	test('FPS measurement returns reasonable value', async ({ waffle }) => {
		const result = await measureFPS(waffle.page, 1000);

		// Verify structure
		expect(result).toHaveProperty('avg');
		expect(result).toHaveProperty('min');
		expect(result).toHaveProperty('frames');
		expect(result).toHaveProperty('duration');

		// Even headless SwiftShader should produce some frames
		expect(result.avg).toBeGreaterThan(0);
		expect(result.frames).toBeGreaterThan(0);
		expect(result.duration).toBeGreaterThanOrEqual(500);

		// expectFPS should pass with a very low threshold
		expectFPS(result, { min: 1 });
	});

	test('pick latency measurement returns a positive number', async ({ waffle }) => {
		const bounds = await getCanvasBounds(waffle.page);
		expect(bounds).not.toBeNull();

		const latency = await measurePickLatency(
			waffle.page,
			bounds.centerX,
			bounds.centerY
		);

		expect(typeof latency).toBe('number');
		expect(latency).toBeGreaterThan(0);
	});

	test('rebuild time reads from store', async ({ waffle }) => {
		const time = await measureRebuildTime(waffle.page);

		expect(typeof time).toBe('number');
		expect(time).toBeGreaterThanOrEqual(0);
	});
});
