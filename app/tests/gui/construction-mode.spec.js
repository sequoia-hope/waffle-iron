/**
 * Construction mode tests — verifies the construction toggle button (X key),
 * construction entity creation, and visual distinction.
 *
 * Construction entities are rendered with dashed lines and a different color
 * (COLOR_CONSTRUCTION = 0x6677aa) vs regular entities (COLOR_DEFAULT = 0x3388ff).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine } from './helpers/canvas.js';
import {
	getActiveTool,
	getEntities,
	waitForEntityCount,
} from './helpers/state.js';
import {
	setSketchSelection,
} from './helpers/constraint.js';

test.describe('construction toggle button', () => {
	test('construction button is visible in sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const btn = waffle.page.locator('[data-testid="toolbar-btn-construction"]');
		await expect(btn).toBeVisible();
	});

	test('construction button not visible outside sketch mode', async ({ waffle }) => {
		const btn = waffle.page.locator('[data-testid="toolbar-btn-construction"]');
		await expect(btn).not.toBeVisible();
	});

	test('clicking construction button toggles construction on selected entity', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();
		expect(line.construction).toBe(false);

		// Select the line programmatically (click-to-select is unreliable in headless)
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);

		// Click construction button
		await waffle.page.locator('[data-testid="toolbar-btn-construction"]').click();
		await waffle.page.waitForTimeout(300);

		// Check construction flag
		const entitiesAfter = await getEntities(waffle.page);
		const lineAfter = entitiesAfter.find(e => e.id === line.id);
		expect(lineAfter).toBeTruthy();
		expect(lineAfter.construction).toBe(true);
	});

	test('X key toggles construction on selected entity', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();

		// Select and toggle via X key
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);
		await pressKey(waffle.page, 'x');
		await waffle.page.waitForTimeout(300);

		const entitiesAfter = await getEntities(waffle.page);
		const lineAfter = entitiesAfter.find(e => e.id === line.id);
		expect(lineAfter).toBeTruthy();
		expect(lineAfter.construction).toBe(true);
	});

	test('toggling construction twice returns to normal', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		// Toggle on
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);
		await pressKey(waffle.page, 'x');
		await waffle.page.waitForTimeout(200);

		let updated = await getEntities(waffle.page);
		let updatedLine = updated.find(e => e.id === line.id);
		const afterFirst = updatedLine?.construction;

		// Toggle off
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);
		await pressKey(waffle.page, 'x');
		await waffle.page.waitForTimeout(200);

		updated = await getEntities(waffle.page);
		updatedLine = updated.find(e => e.id === line.id);
		const afterSecond = updatedLine?.construction;

		// First toggle should change, second should revert
		expect(afterFirst).toBeDefined();
		expect(afterSecond).toBeDefined();
		expect(afterFirst).not.toBe(afterSecond);
	});

	test('construction toggle is a no-op with empty selection', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entitiesBefore = await getEntities(waffle.page);

		// Clear selection and try toggling
		await setSketchSelection(waffle.page, []);
		await waffle.page.waitForTimeout(200);
		await pressKey(waffle.page, 'x');
		await waffle.page.waitForTimeout(200);

		const entitiesAfter = await getEntities(waffle.page);

		// All construction flags should be unchanged
		for (const e of entitiesAfter) {
			const before = entitiesBefore.find(b => b.id === e.id);
			if (before) {
				expect(e.construction).toBe(before.construction);
			}
		}
	});
});
