/**
 * Infrastructure tests — trace replay system.
 *
 * Verifies that the trace replay helper can execute click, drag, and key
 * actions and optionally assert __waffle state after each step.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { replayTrace } from '../helpers/trace.js';
import { getCanvasBounds } from '../helpers/canvas.js';

test.describe('trace replay infrastructure', () => {
	test('replay a simple click trace verifies state', async ({ waffle }) => {
		// Find the sketch button so we can build a trace with absolute coordinates
		const sketchBtn = waffle.page.locator('[data-testid="toolbar-btn-sketch"]');
		const box = await sketchBtn.boundingBox();
		expect(box).not.toBeNull();

		const btnX = box.x + box.width / 2;
		const btnY = box.y + box.height / 2;

		const trace = [
			{
				action: 'click',
				params: [btnX, btnY],
				assert: {
					fn: 'getState',
					path: 'sketchMode.active',
					expected: true,
				},
			},
		];

		await replayTrace(waffle.page, trace);

		// Double-check via direct evaluation
		const active = await waffle.page.evaluate(
			() => window.__waffle?.getState()?.sketchMode?.active ?? false
		);
		expect(active).toBe(true);
	});

	test('trace with orbit drag replays deterministically', async ({ waffle }) => {
		const bounds = await getCanvasBounds(waffle.page);
		expect(bounds).not.toBeNull();

		const cx = bounds.centerX;
		const cy = bounds.centerY;

		const trace = [
			{
				action: 'drag',
				params: [cx, cy, cx + 100, cy + 50],
			},
		];

		await replayTrace(waffle.page, trace);

		// Canvas should still be visible and responsive after the drag
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('trace with key press replays correctly', async ({ waffle }) => {
		const trace = [
			{
				action: 'key',
				params: ['f'],
			},
			{
				action: 'wait',
				params: [300],
			},
		];

		await replayTrace(waffle.page, trace);

		// No crash — canvas is still visible after fit-all key press
		const canvas = waffle.page.locator('canvas');
		await expect(canvas).toBeVisible();
	});
});
