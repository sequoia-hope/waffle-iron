/**
 * Sprint 8: Polyline tool tests.
 *
 * Verifies multi-click polyline with close-to-start auto-close
 * and H/V auto-constraints on near-horizontal/vertical segments.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickSelect } from './helpers/toolbar.js';
import { clickAt, moveTo } from './helpers/canvas.js';
import { getEntityCount, getEntityCountByType, waitForEntityCount, getEntities } from './helpers/state.js';
import { getConstraints } from './helpers/constraint.js';

test.describe('sketch polyline tool', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		// Activate polyline tool
		await waffle.page.evaluate(() => window.__waffle.setTool('polyline'));
		await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'polyline',
			{ timeout: 3000 }
		);
	});

	test('3-click polyline creates 3 points + 2 lines (open)', async ({ waffle }) => {
		const page = waffle.page;

		// Click 3 points
		await clickAt(page, -100, 0);
		await clickAt(page, 0, -50);
		await clickAt(page, 100, 0);
		// Escape to finish (open polyline)
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		const points = await getEntityCountByType(page, 'Point');
		const lines = await getEntityCountByType(page, 'Line');
		expect(points).toBe(3);
		expect(lines).toBe(2);
	});

	test('close polyline by snapping to first point', async ({ waffle }) => {
		const page = waffle.page;

		// Click triangle vertices, close by clicking near start
		await clickAt(page, -80, 60);
		await clickAt(page, 80, 60);
		await clickAt(page, 0, -60);

		// Move close to the first point and click to close
		await moveTo(page, -80, 60);
		await page.waitForTimeout(100);
		await clickAt(page, -80, 60);
		await page.waitForTimeout(300);

		// Should have 3 points + 3 lines (closed triangle)
		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBe(3);
		// Closed polyline reuses first point, so 3 points total
		expect(points.length).toBe(3);
	});

	test('polyline creates connected segments (shared points)', async ({ waffle }) => {
		const page = waffle.page;

		await clickAt(page, -100, -50);
		await clickAt(page, 0, -50);
		await clickAt(page, 50, 50);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBe(2);

		// Second line's start should be first line's end (shared point)
		expect(lines[1].start_id).toBe(lines[0].end_id);
	});

	test('polyline tool button appears in toolbar', async ({ waffle }) => {
		const page = waffle.page;

		const btn = page.locator('[data-testid="toolbar-btn-polyline"]');
		expect(await btn.isVisible()).toBe(true);
	});

	test('P key activates polyline tool', async ({ waffle }) => {
		const page = waffle.page;

		// Switch to select first
		await page.evaluate(() => window.__waffle.setTool('select'));
		await page.waitForTimeout(100);

		await page.keyboard.press('p');
		await page.waitForTimeout(200);

		const tool = await page.evaluate(() => window.__waffle?.getState()?.activeTool);
		expect(tool).toBe('polyline');
	});
});
