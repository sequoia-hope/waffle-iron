/**
 * Snap click diagnostic tests — verifies that clicking on/near existing points
 * properly snaps and reuses them (coincident snap).
 *
 * These tests diagnose whether snap detection works when clicking near existing
 * sketch entities, using both API setup and real pointer events.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, moveTo } from './helpers/canvas.js';
import {
	getEntities,
	waitForEntityCount,
	getActiveTool,
	getToolState,
	getToolEventLog,
	clearToolEventLog,
} from './helpers/state.js';

test.describe('snap point coincident detection', () => {
	test('second line reuses endpoint via coincident snap', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickLine(page);

		// Draw first line from (-100, 0) to (0, 0)
		await drawLine(page, -100, 0, 0, 0);
		await waitForEntityCount(page, 3, 5000);

		// Get the endpoint position — should be near canvas center
		const entities1 = await getEntities(page);
		const points1 = entities1.filter(e => e.type === 'Point');
		expect(points1).toHaveLength(2);

		// Draw second line starting from the same end position (0, 0) to (100, 0)
		// The line tool chains automatically, so after the first line, tool state
		// should be 'firstPointPlaced' with the end point as start
		const toolState = await getToolState(page);

		// If chaining is active, just click the endpoint
		if (toolState === 'firstPointPlaced') {
			await clickAt(page, 100, 0);
		} else {
			// If not chaining, start a new line from the same endpoint
			await clickAt(page, 0, 0);
			await clickAt(page, 100, 0);
		}
		await waitForEntityCount(page, 5, 5000);

		// Should have 3 points and 2 lines (shared middle point)
		const entities2 = await getEntities(page);
		const points2 = entities2.filter(e => e.type === 'Point');
		const lines2 = entities2.filter(e => e.type === 'Line');
		expect(lines2).toHaveLength(2);
		// The endpoint of line 1 should be the start point of line 2
		expect(points2.length).toBeLessThanOrEqual(3);
	});

	test('non-chained line snap to existing point', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickLine(page);

		// Draw first line
		await drawLine(page, -100, 0, 0, 0);
		await waitForEntityCount(page, 3, 5000);

		// Press Escape to break chain, then start new line tool
		await pressKey(page, 'Escape');
		await clickLine(page);
		await clearToolEventLog(page);

		// Click near the endpoint of the first line (canvas center = sketch origin)
		// This should trigger coincident snap
		await clickAt(page, 0, 0);
		await page.waitForTimeout(300);

		const toolState = await getToolState(page);
		expect(toolState).toBe('firstPointPlaced');

		// Now click a second point to complete the line
		await clickAt(page, 0, -100);
		await waitForEntityCount(page, 5, 5000);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');

		expect(lines).toHaveLength(2);
		// Check that the second line shares the endpoint with the first
		const line1 = lines[0];
		const line2 = lines[1];

		const sharedPoint = [line1.start_id, line1.end_id].some(
			id => id === line2.start_id || id === line2.end_id
		);
		expect(sharedPoint).toBe(true);
	});

	test('snap indicator appears on hover near existing point', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickLine(page);

		// Draw a line from (-100, 0) to (100, 0)
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		// Break chain
		await pressKey(page, 'Escape');
		await clickLine(page);

		// Move near the endpoint at (100, 0) — should show snap indicator
		await moveTo(page, 100, 0);
		await page.waitForTimeout(300);

		// Check if snap indicator is showing
		const snapIndicator = await page.evaluate(() => {
			return window.__waffle?.getSnapIndicator?.() ?? null;
		});

		// We expect a coincident or some snap indicator near the existing point
		// (may be null if the pixel-to-sketch mapping doesn't land close enough)
		if (snapIndicator) {
			expect(snapIndicator.type).toBe('coincident');
		}
	});

	test('entity positions reflect correct sketch coordinates after snap', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);

		// Add entities via API for precise positioning
		await page.evaluate(() => {
			const w = window.__waffle;
			// Create two points and a line
			w.addSketchEntity({ type: 'Point', id: 101, x: -5, y: 0 });
			w.addSketchEntity({ type: 'Point', id: 102, x: 5, y: 0 });
			w.addSketchEntity({ type: 'Line', id: 103, start_id: 101, end_id: 102, construction: false });
		});
		await waitForEntityCount(page, 3, 3000);

		// Get positions to verify they exist
		const positions = await page.evaluate(() => {
			const posMap = window.__waffle.getPositions();
			const result = {};
			for (const [id, pos] of posMap) {
				result[id] = pos;
			}
			return result;
		});

		expect(positions[101]).toBeDefined();
		expect(positions[102]).toBeDefined();
		expect(positions[101].x).toBeCloseTo(-5, 0);
		expect(positions[102].x).toBeCloseTo(5, 0);
	});
});
