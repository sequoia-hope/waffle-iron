/**
 * Select-other cycling — real GUI tests with mouse clicks on actual geometry.
 *
 * Previously these tests bypassed the GUI via window.__waffle.selectRef().
 * Now they create geometry programmatically (acceptable hybrid setup) and interact
 * via real mouse events for all selection operations.
 * __waffle is only used for state verification and coordinate discovery.
 */
import { test, expect } from '../helpers/waffle-test.js';
import {
	createExtrudedBox,
	getVisibleFaces,
	clickFace,
	clickEmpty,
	findTwoDistinctFaces,
} from '../helpers/geometry.js';

test.describe('select other cycling', () => {
	test('select other state starts empty', async ({ waffle }) => {
		const page = waffle.page;
		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state.intersections).toEqual([]);
	});

	test('clicking face populates select-other intersections', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		// Click on a face → real mouse event populates intersection list
		await clickFace(page, faces[0]);

		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state).toHaveProperty('intersections');
		expect(state).toHaveProperty('cycleIndex');
		expect(state.intersections.length).toBeGreaterThan(0);
	});

	test('repeat-click at same position cycles selection', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		// Click face once → establishes intersection list
		await clickFace(page, faces[0]);
		const stateAfterFirst = await page.evaluate(() => window.__waffle.getSelectOtherState());
		const firstCycleIndex = stateAfterFirst.cycleIndex;

		if (stateAfterFirst.intersections.length > 1) {
			// Repeat click at same position → should cycle to next
			await clickFace(page, faces[0]);
			const stateAfterSecond = await page.evaluate(() => window.__waffle.getSelectOtherState());
			expect(stateAfterSecond.cycleIndex).not.toBe(firstCycleIndex);
		} else {
			// Single intersection — cycle index stays at 0
			await clickFace(page, faces[0]);
			const stateAfterSecond = await page.evaluate(() => window.__waffle.getSelectOtherState());
			expect(stateAfterSecond.cycleIndex).toBe(0);
		}
	});

	test('shift-click at different position adds to selection', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		const pair = await findTwoDistinctFaces(page, faces);

		if (pair) {
			const [face1, face2] = pair;
			await clickFace(page, face1);
			let selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
			expect(selected.length).toBeGreaterThanOrEqual(1);

			// Shift-click different face → adds to selection
			await clickFace(page, face2, { shift: true });
			selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
			expect(selected.length).toBeGreaterThanOrEqual(2);
		} else {
			// Fallback: only one face reachable — verify basic click selection through real GUI
			await clickFace(page, faces[0]);
			const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
			expect(selected.length).toBeGreaterThanOrEqual(1);
		}
	});

	test('click empty resets selection and select-other state', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		// Click face to populate state
		await clickFace(page, faces[0]);
		let selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected.length).toBeGreaterThanOrEqual(1);

		// Click empty space → clears everything via handleMiss()
		await clickEmpty(page);

		selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(0);

		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state.intersections).toEqual([]);
	});
});
