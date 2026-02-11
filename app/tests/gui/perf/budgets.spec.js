/**
 * Performance budget tests.
 *
 * These verify that the app meets minimum performance thresholds
 * even in headless (SwiftShader) mode. Thresholds are deliberately
 * lenient to avoid flaky failures on slow CI runners.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { measureFPS, measurePickLatency, measureRebuildTime, expectFPS, expectLatency } from '../helpers/perf.js';
import { getCanvasBounds, clickAt } from '../helpers/canvas.js';

test.describe('performance budgets', () => {
	test('FPS meets minimum budget (>=10 avg in headless)', async ({ waffle }) => {
		const result = await measureFPS(waffle.page, 2000);

		expect(result.avg).toBeGreaterThan(0);
		expect(result.frames).toBeGreaterThan(0);

		// Headless SwiftShader should still manage >=10 FPS with a simple scene
		expectFPS(result, { min: 10 });
	});

	test('face pick latency within budget (p95 <= 200ms)', async ({ waffle }) => {
		const bounds = await getCanvasBounds(waffle.page);
		expect(bounds).not.toBeNull();

		// Measure pick latency at canvas center (where test box lives)
		// Note: includes waitForFunction polling overhead + IPC, so budget
		// is generous to handle CI parallel worker contention
		const latency = await measurePickLatency(
			waffle.page,
			bounds.centerX,
			bounds.centerY
		);

		expect(typeof latency).toBe('number');
		expectLatency(latency, { p95: 200 });
	});

	test('edge pick latency within budget (p95 <= 200ms)', async ({ waffle }) => {
		const bounds = await getCanvasBounds(waffle.page);
		expect(bounds).not.toBeNull();

		// Move near the edge of the test box area
		const edgeX = bounds.centerX + 30;
		const edgeY = bounds.centerY + 30;

		const latency = await measurePickLatency(waffle.page, edgeX, edgeY);

		expect(typeof latency).toBe('number');
		expectLatency(latency, { p95: 200 });
	});

	test('rebuild time within budget (<= 500ms)', async ({ waffle }) => {
		const time = await measureRebuildTime(waffle.page);

		expect(typeof time).toBe('number');
		expect(time).toBeLessThanOrEqual(500);
	});

	test('sketch solve latency within budget (<= 100ms)', async ({ waffle }) => {
		// Enter sketch mode
		const sketchBtn = waffle.page.locator('[data-testid="toolbar-btn-sketch"]');
		if (await sketchBtn.isVisible()) {
			await sketchBtn.click();
			await waffle.page.waitForTimeout(300);
		}

		// Measure solve time via rebuild proxy
		const time = await measureRebuildTime(waffle.page);
		expect(typeof time).toBe('number');
		expect(time).toBeLessThanOrEqual(100);
	});
});
