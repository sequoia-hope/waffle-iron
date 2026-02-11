/**
 * Constraint toolbar tests — verifies constraint buttons enable/disable
 * based on selection and that clicking them creates the correct constraints.
 *
 * Uses setSketchSelection via __waffle API for reliable entity selection,
 * since pixel-perfect click-to-select depends on camera/coordinate mapping.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickLine, clickSelect, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle, drawCircle } from './helpers/canvas.js';
import {
	getEntityCount,
	getEntityCountByType,
	getEntities,
	waitForEntityCount,
	getActiveTool,
} from './helpers/state.js';
import {
	clickConstraintButton,
	isConstraintEnabled,
	isConstraintVisible,
	setSketchSelection,
	getSketchSelection,
	getConstraints,
	getConstraintCount,
	getConstraintCountByType,
	waitForConstraintCount,
} from './helpers/constraint.js';

/**
 * Helper: enter sketch mode and draw a line using GUI events.
 */
async function sketchWithLine(waffle) {
	await clickSketch(waffle.page);
	await drawLine(waffle.page, -100, 0, 100, 0);
	try {
		await waitForEntityCount(waffle.page, 3, 3000);
	} catch {
		await waffle.dumpState('sketchWithLine-failed');
	}
}

/**
 * Helper: enter sketch mode, draw two separate lines.
 */
async function sketchWithTwoLines(waffle) {
	await clickSketch(waffle.page);
	await drawLine(waffle.page, -100, -50, 0, -50);
	try {
		await waitForEntityCount(waffle.page, 3, 3000);
	} catch {}

	await pressKey(waffle.page, 'Escape');
	await pressKey(waffle.page, 'l');

	await drawLine(waffle.page, -100, 80, 0, 80);
	try {
		await waitForEntityCount(waffle.page, 6, 3000);
	} catch {
		await waffle.dumpState('sketchWithTwoLines-failed');
	}
}

/**
 * Helper: enter sketch and draw a rectangle.
 */
async function sketchWithRectangle(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('sketchWithRectangle-failed');
	}
}

test.describe('constraint button visibility', () => {
	test('constraint buttons are visible in sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		for (const id of ['horizontal', 'vertical', 'coincident', 'perpendicular',
			'parallel', 'equal', 'tangent', 'midpoint', 'fix']) {
			const visible = await isConstraintVisible(waffle.page, id);
			expect(visible, `${id} button should be visible`).toBe(true);
		}
	});

	test('constraint buttons are NOT visible outside sketch mode', async ({ waffle }) => {
		for (const id of ['horizontal', 'vertical', 'coincident']) {
			const visible = await isConstraintVisible(waffle.page, id);
			expect(visible, `${id} button should NOT be visible outside sketch`).toBe(false);
		}
	});
});

test.describe('constraint button enable/disable state', () => {
	test('all constraint buttons disabled when nothing selected', async ({ waffle }) => {
		await clickSketch(waffle.page);

		for (const id of ['horizontal', 'vertical', 'coincident', 'perpendicular',
			'parallel', 'equal', 'tangent', 'midpoint', 'fix']) {
			const enabled = await isConstraintEnabled(waffle.page, id);
			expect(enabled, `${id} should be disabled with no selection`).toBe(false);
		}
	});

	test('H/V enabled when one line is selected', async ({ waffle }) => {
		await sketchWithLine(waffle);

		// Switch to select tool
		await pressKey(waffle.page, 'Escape');

		// Programmatically select the line
		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');
		expect(line, 'should have a Line entity').toBeTruthy();

		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);

		// H and V should be enabled for a single line
		const hEnabled = await isConstraintEnabled(waffle.page, 'horizontal');
		const vEnabled = await isConstraintEnabled(waffle.page, 'vertical');
		expect(hEnabled, 'horizontal should be enabled for a line').toBe(true);
		expect(vEnabled, 'vertical should be enabled for a line').toBe(true);
	});

	test('fix enabled when one point is selected', async ({ waffle }) => {
		await sketchWithLine(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const point = entities.find(e => e.type === 'Point');
		expect(point, 'should have a Point entity').toBeTruthy();

		await setSketchSelection(waffle.page, [point.id]);
		await waffle.page.waitForTimeout(200);

		const fixEnabled = await isConstraintEnabled(waffle.page, 'fix');
		expect(fixEnabled, 'fix should be enabled for a point').toBe(true);
	});

	test('parallel enabled when two lines are selected', async ({ waffle }) => {
		await sketchWithTwoLines(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBeGreaterThanOrEqual(2);

		await setSketchSelection(waffle.page, [lines[0].id, lines[1].id]);
		await waffle.page.waitForTimeout(200);

		const parallelEnabled = await isConstraintEnabled(waffle.page, 'parallel');
		expect(parallelEnabled, 'parallel should be enabled for two lines').toBe(true);
	});

	test('coincident enabled when two points are selected', async ({ waffle }) => {
		await sketchWithTwoLines(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBeGreaterThanOrEqual(2);

		await setSketchSelection(waffle.page, [points[0].id, points[1].id]);
		await waffle.page.waitForTimeout(200);

		const coincidentEnabled = await isConstraintEnabled(waffle.page, 'coincident');
		expect(coincidentEnabled, 'coincident should be enabled for two points').toBe(true);
	});

	test('buttons re-disable when selection is cleared', async ({ waffle }) => {
		await sketchWithLine(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		// Select line -> H should be enabled
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);
		expect(await isConstraintEnabled(waffle.page, 'horizontal')).toBe(true);

		// Clear selection -> H should be disabled
		await setSketchSelection(waffle.page, []);
		await waffle.page.waitForTimeout(200);
		expect(await isConstraintEnabled(waffle.page, 'horizontal')).toBe(false);
	});
});

test.describe('constraint application via toolbar buttons', () => {
	test('apply horizontal constraint to a line via toolbar button', async ({ waffle }) => {
		await sketchWithLine(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();

		// Select the line programmatically
		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);

		const constraintsBefore = await getConstraintCount(waffle.page);

		// Click the H constraint button
		await clickConstraintButton(waffle.page, 'horizontal');

		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore + 1);

		const constraints = await getConstraints(waffle.page);
		const hConstraint = constraints.find(c => c.type === 'Horizontal');
		expect(hConstraint, 'should have created a Horizontal constraint').toBeTruthy();
	});

	test('apply vertical constraint to a line via toolbar button', async ({ waffle }) => {
		await sketchWithLine(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		await setSketchSelection(waffle.page, [line.id]);
		await waffle.page.waitForTimeout(200);

		const constraintsBefore = await getConstraintCount(waffle.page);
		await clickConstraintButton(waffle.page, 'vertical');

		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore + 1);

		const constraints = await getConstraints(waffle.page);
		const vConstraint = constraints.find(c => c.type === 'Vertical');
		expect(vConstraint, 'should have created a Vertical constraint').toBeTruthy();
	});

	test('apply fix constraint to a point via toolbar button', async ({ waffle }) => {
		await sketchWithLine(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const point = entities.find(e => e.type === 'Point');

		await setSketchSelection(waffle.page, [point.id]);
		await waffle.page.waitForTimeout(200);

		const constraintsBefore = await getConstraintCount(waffle.page);
		await clickConstraintButton(waffle.page, 'fix');

		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore + 1);
	});

	test('apply parallel constraint to two lines via toolbar button', async ({ waffle }) => {
		await sketchWithTwoLines(waffle);
		await pressKey(waffle.page, 'Escape');

		const entities = await getEntities(waffle.page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBeGreaterThanOrEqual(2);

		await setSketchSelection(waffle.page, [lines[0].id, lines[1].id]);
		await waffle.page.waitForTimeout(200);

		const constraintsBefore = await getConstraintCount(waffle.page);
		await clickConstraintButton(waffle.page, 'parallel');

		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore + 1);

		const constraints = await getConstraints(waffle.page);
		const parallelConstraint = constraints.find(c => c.type === 'Parallel');
		expect(parallelConstraint, 'should have created a Parallel constraint').toBeTruthy();
	});

	test('draw rectangle auto-creates H/V constraints', async ({ waffle }) => {
		await sketchWithRectangle(waffle);

		const hCount = await getConstraintCountByType(waffle.page, 'Horizontal');
		const vCount = await getConstraintCountByType(waffle.page, 'Vertical');

		expect(hCount).toBe(2);
		expect(vCount).toBe(2);
	});

	test('constraint count increases after auto-application', async ({ waffle }) => {
		await sketchWithRectangle(waffle);

		const totalConstraints = await getConstraintCount(waffle.page);
		expect(totalConstraints).toBe(4);

		const constraints = await getConstraints(waffle.page);
		const types = constraints.map(c => c.type);
		expect(types.filter(t => t === 'Horizontal')).toHaveLength(2);
		expect(types.filter(t => t === 'Vertical')).toHaveLength(2);
	});
});

test.describe('constraint toolbar button interaction', () => {
	test('clicking disabled constraint button does nothing', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const constraintsBefore = await getConstraintCount(waffle.page);

		try {
			await waffle.page.locator('[data-testid="toolbar-constraint-horizontal"]').click({ force: true });
		} catch {
			// Expected — button may reject click when disabled
		}
		await waffle.page.waitForTimeout(200);

		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore);
	});

	test('dimension button shows in sketch mode toolbar', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const dimBtn = waffle.page.locator('[data-testid="toolbar-btn-dimension"]');
		await expect(dimBtn).toBeVisible();
	});

	test('clicking dimension button activates dimension tool', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await waffle.page.locator('[data-testid="toolbar-btn-dimension"]').click();
		await waffle.page.waitForTimeout(200);

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('dimension');
	});

	test('D key activates dimension tool', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await pressKey(waffle.page, 'd');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('dimension');
	});
});
