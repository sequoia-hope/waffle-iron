/**
 * Datum plane selection and sketch entry tests.
 *
 * Tests that datum planes can be selected via the __waffle API
 * and that selecting a plane before clicking Sketch enters sketch
 * mode on the correct plane.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { isSketchActive } from './helpers/state.js';

test.describe('datum plane selection', () => {
	test('programmatically selecting XY datum plane sets selectedRefs', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XY' } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(1);
		expect(refs[0].anchor.type).toBe('DatumPlane');
		expect(refs[0].anchor.plane).toBe('XY');
	});

	test('programmatically selecting XZ datum plane sets selectedRefs', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XZ' } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(1);
		expect(refs[0].anchor.plane).toBe('XZ');
	});

	test('programmatically selecting YZ datum plane sets selectedRefs', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'YZ' } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(1);
		expect(refs[0].anchor.plane).toBe('YZ');
	});

	test('clearSelection removes datum plane selection', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XY' } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);
		await waffle.page.evaluate(() => window.__waffle.clearSelection());
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(0);
	});

	test('computeFacePlane returns correct plane for XY datum', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XY' } };

		const plane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			ref
		);

		expect(plane).not.toBeNull();
		expect(plane.origin).toEqual([0, 0, 0]);
		expect(plane.normal).toEqual([0, 0, 1]);
	});

	test('computeFacePlane returns correct plane for XZ datum', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XZ' } };

		const plane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			ref
		);

		expect(plane).not.toBeNull();
		expect(plane.origin).toEqual([0, 0, 0]);
		expect(plane.normal).toEqual([0, 1, 0]);
	});

	test('computeFacePlane returns correct plane for YZ datum', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'YZ' } };

		const plane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			ref
		);

		expect(plane).not.toBeNull();
		expect(plane.origin).toEqual([0, 0, 0]);
		expect(plane.normal).toEqual([1, 0, 0]);
	});
});

test.describe('sketch entry from selected datum plane', () => {
	test('select XY plane then click Sketch enters sketch on XY', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XY' } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([0, 0, 1]);
		expect(state.sketchMode.origin).toEqual([0, 0, 0]);
	});

	test('select XZ plane then click Sketch enters sketch on XZ', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XZ' } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([0, 1, 0]);
	});

	test('select YZ plane then click Sketch enters sketch on YZ', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'YZ' } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([1, 0, 0]);
	});

	test('no selection defaults to XY plane', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([0, 0, 1]);
	});

	test('S key with selected datum plane enters sketch on that plane', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XZ' } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		await waffle.page.keyboard.press('s');

		const active = await waffle.page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		).then(() => true).catch(() => false);

		expect(active).toBe(true);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.normal).toEqual([0, 1, 0]);
	});
});
