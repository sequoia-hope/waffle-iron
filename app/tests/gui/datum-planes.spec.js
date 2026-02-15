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

const FRONT_PLANE_ID = '00000000-0000-0000-0000-000000000001';
const TOP_PLANE_ID = '00000000-0000-0000-0000-000000000002';
const RIGHT_PLANE_ID = '00000000-0000-0000-0000-000000000003';

test.describe('datum plane selection', () => {
	test('programmatically selecting Front datum plane sets selectedRefs', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: FRONT_PLANE_ID } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(1);
		expect(refs[0].anchor.type).toBe('DatumPlane');
		expect(refs[0].anchor.id).toBe(FRONT_PLANE_ID);
	});

	test('programmatically selecting Top datum plane sets selectedRefs', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: TOP_PLANE_ID } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(1);
		expect(refs[0].anchor.id).toBe(TOP_PLANE_ID);
	});

	test('programmatically selecting Right datum plane sets selectedRefs', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: RIGHT_PLANE_ID } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(1);
		expect(refs[0].anchor.id).toBe(RIGHT_PLANE_ID);
	});

	test('clearSelection removes datum plane selection', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: FRONT_PLANE_ID } };

		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);
		await waffle.page.evaluate(() => window.__waffle.clearSelection());
		await waffle.page.waitForTimeout(100);

		const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(refs).toHaveLength(0);
	});

	test('computeFacePlane returns correct plane for Front datum', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: FRONT_PLANE_ID } };

		const plane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			ref
		);

		expect(plane).not.toBeNull();
		expect(plane.origin).toEqual([0, 0, 0]);
		expect(plane.normal).toEqual([0, 0, 1]);
	});

	test('computeFacePlane returns correct plane for Top datum', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: TOP_PLANE_ID } };

		const plane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			ref
		);

		expect(plane).not.toBeNull();
		expect(plane.origin).toEqual([0, 0, 0]);
		expect(plane.normal).toEqual([0, 1, 0]);
	});

	test('computeFacePlane returns correct plane for Right datum', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: RIGHT_PLANE_ID } };

		const plane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			ref
		);

		expect(plane).not.toBeNull();
		expect(plane.origin).toEqual([0, 0, 0]);
		expect(plane.normal).toEqual([1, 0, 0]);
	});

	test('legacy XY format still works via computeFacePlane', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XY' } };

		const plane = await waffle.page.evaluate(
			(r) => window.__waffle.computeFacePlane(r),
			ref
		);

		expect(plane).not.toBeNull();
		expect(plane.origin).toEqual([0, 0, 0]);
		expect(plane.normal).toEqual([0, 0, 1]);
	});

	test('getDatumPlanes returns built-in planes', async ({ waffle }) => {
		const planes = await waffle.page.evaluate(() => window.__waffle.getDatumPlanes());
		expect(planes).toHaveLength(3);
		expect(planes[0].name).toBe('Front');
		expect(planes[1].name).toBe('Top');
		expect(planes[2].name).toBe('Right');
	});
});

test.describe('sketch entry from selected datum plane', () => {
	test('select Front plane then click Sketch enters sketch on XY', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: FRONT_PLANE_ID } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([0, 0, 1]);
		expect(state.sketchMode.origin).toEqual([0, 0, 0]);
	});

	test('select Top plane then click Sketch enters sketch on XZ', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: TOP_PLANE_ID } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([0, 1, 0]);
	});

	test('select Right plane then click Sketch enters sketch on YZ', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: RIGHT_PLANE_ID } };
		await waffle.page.evaluate((r) => window.__waffle.selectRef(r), ref);
		await waffle.page.waitForTimeout(100);

		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([1, 0, 0]);
	});

	test('no selection defaults to Front plane', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const state = await waffle.page.evaluate(() => window.__waffle.getState());
		expect(state.sketchMode.active).toBe(true);
		expect(state.sketchMode.normal).toEqual([0, 0, 1]);
	});

	test('S key with selected datum plane enters sketch on that plane', async ({ waffle }) => {
		const ref = { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id: TOP_PLANE_ID } };
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
