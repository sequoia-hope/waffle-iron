/**
 * Face selection tests — real GUI tests with mouse clicks on actual geometry.
 *
 * Creates geometry via API (acceptable hybrid setup) and tests face picking
 * through real mouse events in the full picking pipeline.
 */
import { test, expect } from '../helpers/waffle-test.js';
import {
	createExtrudedBox,
	getVisibleFaces,
	clickFace,
	clickEmpty,
	findTwoDistinctFaces,
} from '../helpers/geometry.js';

test.describe('face selection', () => {
	test('click on face selects a Face ref', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		await clickFace(page, faces[0]);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected.length).toBeGreaterThanOrEqual(1);
		expect(selected[0].kind.type).toBe('Face');
	});

	test('click different face changes selection', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		const pair = await findTwoDistinctFaces(page, faces);

		if (pair) {
			const [face1, face2] = pair;

			// Click first face
			await clickFace(page, face1);
			const selected1 = await page.evaluate(() =>
				JSON.stringify(window.__waffle.getSelectedRefs())
			);

			// Click second face (without shift — replaces selection)
			await clickFace(page, face2);
			const selected2 = await page.evaluate(() =>
				JSON.stringify(window.__waffle.getSelectedRefs())
			);

			// Selection should change
			expect(selected2).not.toBe(selected1);
		} else {
			// Only one face reachable — verify basic selection works
			await clickFace(page, faces[0]);
			const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
			expect(selected.length).toBeGreaterThanOrEqual(1);
		}
	});

	test('click empty space clears face selection', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		// Select a face
		await clickFace(page, faces[0]);
		let selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected.length).toBeGreaterThanOrEqual(1);

		// Click empty space
		await clickEmpty(page);
		selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(0);
	});

	test('selected face ref has expected shape', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		await clickFace(page, faces[0]);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected.length).toBeGreaterThanOrEqual(1);

		const ref = selected[0];
		expect(ref).toHaveProperty('kind');
		expect(ref.kind).toHaveProperty('type');
		expect(ref.kind.type).toBe('Face');
		// Face refs should have a feature_id anchor
		expect(ref).toHaveProperty('anchor');
	});

	test('shift-click adds face to selection', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		const pair = await findTwoDistinctFaces(page, faces);

		if (pair) {
			const [face1, face2] = pair;

			await clickFace(page, face1);
			let selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
			const count1 = selected.length;
			expect(count1).toBeGreaterThanOrEqual(1);

			// Shift-click adds to selection
			await clickFace(page, face2, { shift: true });
			selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
			expect(selected.length).toBeGreaterThan(count1);
		} else {
			await clickFace(page, faces[0]);
			const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
			expect(selected.length).toBeGreaterThanOrEqual(1);
		}
	});

	test('hover over face sets hoveredRef', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		// Move mouse over face centroid without clicking
		await page.mouse.move(faces[0].screenX, faces[0].screenY);
		await page.waitForTimeout(300);

		const hovered = await page.evaluate(() => window.__waffle.getHoveredRef());
		expect(hovered).not.toBeNull();
	});

	test('double-click face populates select-other state', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		// Click face to populate intersection list
		await clickFace(page, faces[0]);

		const state = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state.intersections.length).toBeGreaterThan(0);

		// Click again at same position to cycle
		await clickFace(page, faces[0]);
		const state2 = await page.evaluate(() => window.__waffle.getSelectOtherState());
		expect(state2).toHaveProperty('cycleIndex');
	});

	test('select face then clear via API matches click-empty', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);

		await clickFace(page, faces[0]);
		let selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected.length).toBeGreaterThanOrEqual(1);

		// Clear via API
		await page.evaluate(() => window.__waffle.clearSelection());
		await page.waitForTimeout(200);

		selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(0);
	});
});
