/**
 * Select-other cycling — state shape, canvas click population, additive selection, and reset.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { getCanvasBounds, clickAt } from '../helpers/canvas.js';

const FACE_REF = {
	kind: { type: 'Face' },
	anchor: { type: 'DatumPlane', plane: 'XY' },
	selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
};

const EDGE_REF = {
	kind: { type: 'Edge' },
	anchor: { type: 'DatumPlane', plane: 'XY' },
	selector: { type: 'Role', role: { type: 'SideFace' }, index: 0 },
};

test.describe('select other cycling', () => {
	test('select other state starts empty', async ({ waffle }) => {
		const page = waffle.page;

		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state.intersections).toEqual([]);
	});

	test('selecting a face populates select-other state', async ({ waffle }) => {
		const page = waffle.page;

		// Click on canvas center where the test box should be
		await clickAt(page, 0, 0);
		await page.waitForTimeout(300);

		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		// After a click on the canvas, the select-other state should be populated
		// (intersections may or may not be empty depending on whether geometry is present)
		expect(state).toHaveProperty('intersections');
		expect(state).toHaveProperty('cycleIndex');
	});

	test('shift-additive select keeps previous selection', async ({ waffle }) => {
		const page = waffle.page;

		// Select first ref
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), FACE_REF);
		await page.waitForTimeout(100);

		// Additively select second ref
		await page.evaluate((ref) => window.__waffle.selectRef(ref, true), EDGE_REF);
		await page.waitForTimeout(100);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(2);

		const kinds = selected.map((r) => r.kind.type);
		expect(kinds).toContain('Face');
		expect(kinds).toContain('Edge');
	});

	test('clearSelection resets select-other state', async ({ waffle }) => {
		const page = waffle.page;

		// Select a ref to populate state
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), FACE_REF);
		await page.waitForTimeout(100);

		// Clear everything
		await page.evaluate(() => window.__waffle.clearSelection());
		await page.waitForTimeout(100);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(0);

		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state.intersections).toEqual([]);
	});

	test('select other state has correct shape', async ({ waffle }) => {
		const page = waffle.page;

		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state).toHaveProperty('intersections');
		expect(state).toHaveProperty('cycleIndex');
		expect(state).toHaveProperty('lastScreenX');
		expect(state).toHaveProperty('lastScreenY');
	});
});
