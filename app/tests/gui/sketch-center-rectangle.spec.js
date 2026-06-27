/**
 * Center-rectangle tool + Rect split-button tests.
 * Spec: specs/center_rectangle.md (and point_pair_horizontal_vertical.md).
 *
 * Drawing uses real pointer events (per the GUI test rules). Structure is read
 * back via __waffle.getEntities()/getConstraints()/getPositions(). The point-pair
 * constraints are proved to BIND by the Rust solver tests; here we assert the
 * tool emits exactly the right entities + constraint set and the center sits at
 * the centroid.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickCenterRectangle } from './helpers/toolbar.js';
import { drawRectangle, dragRectangle } from './helpers/canvas.js';
import { getEntityCountByType, waitForEntityCount, getEntities } from './helpers/state.js';

const getConstraints = (page) => page.evaluate(() => window.__waffle.getConstraints());
const getPositions = (page) =>
	page.evaluate(() => Array.from(window.__waffle.getPositions().entries()));

function countByType(constraints, type) {
	return constraints.filter((c) => c.type === type).length;
}

test.describe('Rect split button', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('dropdown selects center mode and persists; R reactivates it', async ({ waffle }) => {
		const page = waffle.page;

		await clickCenterRectangle(page);
		let state = await page.evaluate(() => window.__waffle.getState());
		expect(state.activeTool).toBe('rectangle-center');
		expect(state.rectMode).toBe('rectangle-center');

		// Switch to another tool, then press R: it must re-activate the persisted
		// rectangle mode (center), not the corner default.
		await page.locator('[data-testid="toolbar-btn-line"]').click();
		await page.keyboard.press('r');
		state = await page.evaluate(() => window.__waffle.getState());
		expect(state.activeTool).toBe('rectangle-center');

		// And the main button face now launches center mode directly.
		await page.locator('[data-testid="toolbar-btn-line"]').click();
		await page.locator('[data-testid="toolbar-btn-rectangle"]').click();
		state = await page.evaluate(() => window.__waffle.getState());
		expect(state.activeTool).toBe('rectangle-center');
	});

	test('corner mode still produces 4 points + 4 lines + 4 H/V edge constraints', async ({ waffle }) => {
		const page = waffle.page;
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);

		expect(await getEntityCountByType(page, 'Point')).toBe(4);
		expect(await getEntityCountByType(page, 'Line')).toBe(4);

		const cons = await getConstraints(page);
		expect(countByType(cons, 'Horizontal')).toBe(2);
		expect(countByType(cons, 'Vertical')).toBe(2);
		// Corner mode adds no center scaffolding.
		expect(countByType(cons, 'Midpoint')).toBe(0);
		expect(countByType(cons, 'VerticalPoints')).toBe(0);
		expect(countByType(cons, 'HorizontalPoints')).toBe(0);
	});
});

test.describe('Center rectangle drawing', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickCenterRectangle(waffle.page);
	});

	test('click-click: 7 points (4 corner + center + 2 mids) + 4 lines + center scheme', async ({ waffle }) => {
		const page = waffle.page;
		// center at canvas origin, corner up-right
		await drawRectangle(page, 0, 0, 80, 60);
		await waitForEntityCount(page, 11, 5000); // 5 real pts + 2 constr pts + 4 lines

		expect(await getEntityCountByType(page, 'Point')).toBe(7);
		expect(await getEntityCountByType(page, 'Line')).toBe(4);

		const cons = await getConstraints(page);
		expect(countByType(cons, 'Horizontal')).toBe(2);
		expect(countByType(cons, 'Vertical')).toBe(2);
		expect(countByType(cons, 'Midpoint')).toBe(2);
		expect(countByType(cons, 'VerticalPoints')).toBe(1);
		expect(countByType(cons, 'HorizontalPoints')).toBe(1);
	});

	test('click-click: center point is non-construction, midpoints are construction (I4)', async ({ waffle }) => {
		const page = waffle.page;
		await drawRectangle(page, 0, 0, 80, 60);
		await waitForEntityCount(page, 11, 5000);

		const entities = await getEntities(page);
		const points = entities.filter((e) => e.type === 'Point');
		const construction = points.filter((p) => p.construction);
		const real = points.filter((p) => !p.construction);
		// 4 corners + center are real; the 2 edge-midpoints are construction.
		expect(real.length).toBe(5);
		expect(construction.length).toBe(2);
	});

	test('click-click: center point sits at the centroid of the four corners (I3)', async ({ waffle }) => {
		const page = waffle.page;
		await drawRectangle(page, 0, 0, 80, 60);
		await waitForEntityCount(page, 11, 5000);

		const entities = await getEntities(page);
		const points = entities.filter((e) => e.type === 'Point');
		// Insertion order in center mode: [center, p1, p2, p3, p4, ...mids].
		const centerId = points[0].id;
		const cornerIds = [points[1].id, points[2].id, points[3].id, points[4].id];

		const pos = new Map(await getPositions(page));
		const center = pos.get(centerId);
		const corners = cornerIds.map((id) => pos.get(id));
		expect(center).toBeTruthy();
		corners.forEach((c) => expect(c).toBeTruthy());

		const cx = corners.reduce((s, c) => s + c.x, 0) / 4;
		const cy = corners.reduce((s, c) => s + c.y, 0) / 4;
		// Sketch units; corners span ~tens of units, so 1e-3 is a tight oracle.
		expect(Math.abs(center.x - cx)).toBeLessThan(1e-3);
		expect(Math.abs(center.y - cy)).toBeLessThan(1e-3);
	});

	test('click-drag: 7 points + 4 lines + center scheme', async ({ waffle }) => {
		const page = waffle.page;
		await dragRectangle(page, 0, 0, 80, 60);
		await waitForEntityCount(page, 11, 5000);

		expect(await getEntityCountByType(page, 'Point')).toBe(7);
		expect(await getEntityCountByType(page, 'Line')).toBe(4);

		const cons = await getConstraints(page);
		expect(countByType(cons, 'Midpoint')).toBe(2);
		expect(countByType(cons, 'VerticalPoints')).toBe(1);
		expect(countByType(cons, 'HorizontalPoints')).toBe(1);
	});
});
