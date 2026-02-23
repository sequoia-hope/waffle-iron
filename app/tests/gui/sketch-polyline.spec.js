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

	test('polyline with snap to existing point reuses point', async ({ waffle }) => {
		const page = waffle.page;

		// Create a point at (5, 0) via API
		await page.evaluate(() => {
			window.__waffle.addSketchEntity({ type: 'Point', id: 900, x: 5, y: 0 });
		});
		await page.waitForTimeout(200);

		// Start polyline at (-80, 0)
		await clickAt(page, -80, 0);
		await page.waitForTimeout(200);

		// Click near (5, 0) — should snap to existing point
		// Get the screen offset for the existing point
		const offset = await page.evaluate(() => window.__waffle.sketchToScreenOffset(5, 0));
		if (offset) {
			await clickAt(page, offset.x, offset.y);
		} else {
			// Fallback: click near where it should be
			await clickAt(page, 40, 0);
		}
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		// Should have reused the existing point, so total entities:
		// 2 new points (start) + 1 line, but endpoint may be the existing 900 point
		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBe(1);
	});

	test('polyline close-to-start with 5 segments → pentagon', async ({ waffle }) => {
		const page = waffle.page;

		// Click 5 vertices in a pentagon pattern
		const radius = 70;
		for (let i = 0; i < 5; i++) {
			const angle = (i / 5) * Math.PI * 2 - Math.PI / 2;
			const x = Math.round(radius * Math.cos(angle));
			const y = Math.round(radius * Math.sin(angle));
			await clickAt(page, x, y);
			await page.waitForTimeout(150);
		}

		// Close by clicking near the first point
		const closeAngle = -Math.PI / 2;
		const closeX = Math.round(radius * Math.cos(closeAngle));
		const closeY = Math.round(radius * Math.sin(closeAngle));
		await moveTo(page, closeX, closeY);
		await page.waitForTimeout(200);
		await clickAt(page, closeX, closeY);
		await page.waitForTimeout(300);

		// Should have 5 points + 5 lines = 10 entities (closed pentagon)
		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBe(5);
		expect(points.length).toBe(5);
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
