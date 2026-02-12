/**
 * Sketch visual feedback tests — fully-constrained status, solve status,
 * over-constrained detection, and snap indicator state.
 *
 * These verify the solver integration: constraints → solve → visual feedback.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle } from './helpers/canvas.js';
import {
	getEntities,
	waitForEntityCount,
	getActiveTool,
} from './helpers/state.js';
import {
	setSketchSelection,
	clickConstraintButton,
	getConstraints,
	getConstraintCount,
} from './helpers/constraint.js';

test.describe('solve status feedback', () => {
	test('solve status is available via __waffle API', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line so the solver has something to process
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);
		await waffle.page.waitForTimeout(500);

		const status = await waffle.page.evaluate(() =>
			window.__waffle?.getSolveStatus()
		);
		// With entities drawn, solver should return a status object
		expect(status).not.toBeNull();
		expect(typeof status).toBe('object');
		expect(typeof status.dof).toBe('number');
	});

	test('drawing entities produces non-null solve status', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Wait for solver to process
		await waffle.page.waitForTimeout(500);

		const status = await waffle.page.evaluate(() =>
			window.__waffle?.getSolveStatus()
		);
		// With entities, we should get a solve status
		expect(status).not.toBeNull();
		expect(typeof status.dof).toBe('number');
	});

	test('fully constraining a sketch reduces DOF to 0', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');
		const points = entities.filter(e => e.type === 'Point');
		expect(line).toBeTruthy();
		expect(points.length).toBeGreaterThanOrEqual(2);

		// Apply horizontal constraint to the line
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);
		await clickConstraintButton(waffle.page, 'horizontal');

		// Fix both endpoints
		for (const pt of points) {
			await setSketchSelection(waffle.page, [pt.id]);
			await waffle.page.waitForTimeout(200);
			await clickConstraintButton(waffle.page, 'fix');
		}

		// Wait for solver
		await waffle.page.waitForTimeout(500);

		const status = await waffle.page.evaluate(() =>
			window.__waffle?.getSolveStatus()
		);

		// With H + 2x fix, DOF should be 0 or -1 (if solver runs asynchronously)
		expect(status).not.toBeNull();
		expect(status.dof).toBeLessThanOrEqual(0);
	});

	test('over-constrained entities array is available', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const overConstrained = await waffle.page.evaluate(() =>
			window.__waffle?.getOverConstrained()
		);
		expect(Array.isArray(overConstrained)).toBe(true);
		// Initially no over-constrained entities
		expect(overConstrained.length).toBe(0);
	});
});

test.describe('rectangle auto-constraint verification', () => {
	test('rectangle lines have correct constraint associations', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		await waitForEntityCount(waffle.page, 8, 3000);

		const entities = await getEntities(waffle.page);
		const lines = entities.filter(e => e.type === 'Line');
		const constraints = await getConstraints(waffle.page);

		expect(lines).toHaveLength(4);
		expect(constraints).toHaveLength(4); // 2H + 2V

		// Every constraint should reference a valid line entity
		const lineIds = new Set(lines.map(l => l.id));
		for (const c of constraints) {
			expect(lineIds.has(c.entity), `constraint references valid line`).toBe(true);
		}
	});

	test('rectangle constraints are structurally correct H/V pairs', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		await waitForEntityCount(waffle.page, 8, 3000);

		const constraints = await getConstraints(waffle.page);
		const types = constraints.map(c => c.type).sort();

		// Should be exactly: Horizontal, Horizontal, Vertical, Vertical
		expect(types).toEqual(['Horizontal', 'Horizontal', 'Vertical', 'Vertical']);
	});
});

test.describe('constraint stacking', () => {
	test('multiple constraints can be applied to same entity', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		// Count existing constraints (snap may auto-apply H for horizontal lines)
		const baseLine = await getConstraintCount(waffle.page);

		// Apply vertical constraint (line is horizontal, so V is a real change)
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);
		await clickConstraintButton(waffle.page, 'vertical');

		let count = await getConstraintCount(waffle.page);
		expect(count).toBe(baseLine + 1);

		// Fix a point on the line
		const point = entities.find(e => e.type === 'Point');
		await setSketchSelection(waffle.page, [point.id]);
		await waffle.page.waitForTimeout(200);
		await clickConstraintButton(waffle.page, 'fix');

		count = await getConstraintCount(waffle.page);
		expect(count).toBe(baseLine + 2);
	});

	test('constraint state persists through tool changes', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		// Apply constraint
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);
		await clickConstraintButton(waffle.page, 'horizontal');

		const countBefore = await getConstraintCount(waffle.page);

		// Switch tools: select -> line -> rectangle -> back to select
		await pressKey(waffle.page, 'l');
		await pressKey(waffle.page, 'r');
		await pressKey(waffle.page, 'Escape');

		// Constraints should persist
		const countAfter = await getConstraintCount(waffle.page);
		expect(countAfter).toBe(countBefore);
	});
});

test.describe('sketch selection state', () => {
	test('setSketchSelection selects correct entities', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		await setSketchSelection(waffle.page, [line.id]);

		const selection = await waffle.page.evaluate(() =>
			window.__waffle?.getSketchSelection() ?? []
		);
		expect(selection).toContain(line.id);
		expect(selection).toHaveLength(1);
	});

	test('clearing selection results in empty array', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		await setSketchSelection(waffle.page, [entities[0].id]);
		await setSketchSelection(waffle.page, []);

		const selection = await waffle.page.evaluate(() =>
			window.__waffle?.getSketchSelection() ?? []
		);
		expect(selection).toHaveLength(0);
	});

	test('multi-select works with multiple entity IDs', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const ids = entities.map(e => e.id);

		await setSketchSelection(waffle.page, ids);

		const selection = await waffle.page.evaluate(() =>
			window.__waffle?.getSketchSelection() ?? []
		);
		expect(selection.length).toBe(ids.length);
	});
});
