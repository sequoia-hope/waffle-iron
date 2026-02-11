/**
 * Box selection — state lifecycle, programmatic selection, additive mode, and clearSelection.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { getCanvasBounds, clickAt } from '../helpers/canvas.js';

const FACE_REF_1 = {
	kind: { type: 'Face' },
	anchor: { type: 'DatumPlane', plane: 'XY' },
	selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
};

const FACE_REF_2 = {
	kind: { type: 'Face' },
	anchor: { type: 'DatumPlane', plane: 'XY' },
	selector: { type: 'Role', role: { type: 'SideFace' }, index: 0 },
};

test.describe('box selection', () => {
	test('box select state starts inactive', async ({ waffle }) => {
		const page = waffle.page;

		const boxState = await page.evaluate(() => window.__waffle.getBoxSelectState());
		expect(boxState.active).toBe(false);
	});

	test('programmatic selection clears on clearSelection', async ({ waffle }) => {
		const page = waffle.page;

		// Select a ref
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), FACE_REF_1);
		await page.waitForTimeout(100);

		let selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(1);

		// Clear and verify
		await page.evaluate(() => window.__waffle.clearSelection());
		await page.waitForTimeout(100);

		selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(0);
	});

	test('shift-additive select preserves existing', async ({ waffle }) => {
		const page = waffle.page;

		// Select first face
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), FACE_REF_1);
		await page.waitForTimeout(100);

		// Additively select second face
		await page.evaluate((ref) => window.__waffle.selectRef(ref, true), FACE_REF_2);
		await page.waitForTimeout(100);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(2);
	});

	test('clearSelection empties selectedRefs', async ({ waffle }) => {
		const page = waffle.page;

		// Add two refs
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), FACE_REF_1);
		await page.waitForTimeout(100);
		await page.evaluate((ref) => window.__waffle.selectRef(ref, true), FACE_REF_2);
		await page.waitForTimeout(100);

		let selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(2);

		// Clear all
		await page.evaluate(() => window.__waffle.clearSelection());
		await page.waitForTimeout(100);

		selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(0);
	});

	test('box select state has correct shape', async ({ waffle }) => {
		const page = waffle.page;

		const boxState = await page.evaluate(() => window.__waffle.getBoxSelectState());
		expect(boxState).toHaveProperty('active');
		expect(boxState).toHaveProperty('startX');
		expect(boxState).toHaveProperty('startY');
		expect(boxState).toHaveProperty('endX');
		expect(boxState).toHaveProperty('endY');
		expect(boxState).toHaveProperty('mode');
	});
});
