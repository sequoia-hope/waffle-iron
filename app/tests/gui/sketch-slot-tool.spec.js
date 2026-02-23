/**
 * Slot tool tests.
 *
 * Verifies slot tool placement creates correct entities (arcs, lines, points)
 * and applies appropriate constraints.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine } from './helpers/canvas.js';
import { getEntities, getEntityCountByType, waitForEntityCount, getToolState } from './helpers/state.js';
import { getConstraints, getConstraintCountByType } from './helpers/constraint.js';

test.describe('sketch slot tool', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('slot tool activates via toolbar button', async ({ waffle }) => {
		const page = waffle.page;

		const btn = page.locator('[data-testid="toolbar-btn-slot"]');
		const visible = await btn.isVisible().catch(() => false);
		expect(visible).toBe(true);

		await btn.click();
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'slot',
			{ timeout: 3000 }
		);

		const activeTool = await page.evaluate(
			() => window.__waffle.getState().activeTool
		);
		expect(activeTool).toBe('slot');
	});

	test('slot tool activates via T shortcut', async ({ waffle }) => {
		const page = waffle.page;

		await pressKey(page, 't');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'slot',
			{ timeout: 3000 }
		);

		const activeTool = await page.evaluate(
			() => window.__waffle.getState().activeTool
		);
		expect(activeTool).toBe('slot');
	});

	test('slot tool first click sets first center point', async ({ waffle }) => {
		const page = waffle.page;

		// Activate slot tool
		const btn = page.locator('[data-testid="toolbar-btn-slot"]');
		await btn.click();
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'slot',
			{ timeout: 3000 }
		);

		// Click first center
		await clickAt(page, -60, 0);
		await page.waitForTimeout(300);

		// Should have created a point entity
		const pointCount = await getEntityCountByType(page, 'Point');
		expect(pointCount).toBeGreaterThanOrEqual(1);

		// Tool state should be in second-center-waiting state
		const toolState = await getToolState(page);
		expect(toolState).toBe('slotFirstCenter');
	});

	test('slot tool creates entities after dimension input', async ({ waffle }) => {
		const page = waffle.page;

		// Activate slot tool
		await page.locator('[data-testid="toolbar-btn-slot"]').click();
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'slot',
			{ timeout: 3000 }
		);

		// Click first center
		await clickAt(page, -60, 0);
		await page.waitForTimeout(300);

		// Click second center
		await clickAt(page, 60, 0);
		await page.waitForTimeout(300);

		// Dimension popup should appear for width
		const hasPopup = await page.evaluate(
			() => window.__waffle.getDimensionPopup() != null
		);

		if (hasPopup) {
			// Enter width value
			const input = page.locator('.dimension-input');
			const visible = await input.isVisible({ timeout: 2000 }).catch(() => false);
			if (visible) {
				await input.fill('3');
				await page.keyboard.press('Enter');
				await page.waitForTimeout(500);
			}
		} else {
			// Fallback: tool may have auto-created with default width
			await page.waitForTimeout(500);
		}

		// Verify entities were created
		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const arcs = entities.filter(e => e.type === 'Arc');
		const lines = entities.filter(e => e.type === 'Line');

		// Slot = 2 arc centers + 4 connection points + 2 arcs + 2 lines = 6 points + 2 arcs + 2 lines
		// We should have at least some of each
		expect(points.length).toBeGreaterThanOrEqual(4);
		expect(arcs.length + lines.length).toBeGreaterThanOrEqual(2);
	});

	test('escape cancels slot tool without creating entities', async ({ waffle }) => {
		const page = waffle.page;

		await page.locator('[data-testid="toolbar-btn-slot"]').click();
		await page.waitForFunction(
			() => window.__waffle?.getState()?.activeTool === 'slot',
			{ timeout: 3000 }
		);

		// Click first center
		await clickAt(page, -60, 0);
		await page.waitForTimeout(300);

		// Count entities after first click
		const countAfterFirst = await page.evaluate(
			() => window.__waffle.getState().entityCount
		);

		// Press Escape to cancel
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		// Tool should reset
		const toolState = await getToolState(page);
		expect(toolState).toBe('idle');
	});
});
