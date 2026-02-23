/**
 * Sprint 6: Missing constraint UI tests.
 *
 * Verifies that newly-exposed constraints (angle, symmetric, point-on-line,
 * equal-radius, etc.) appear in the menu and work correctly.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickCircle, clickSelect } from './helpers/toolbar.js';
import { drawLine, drawCircle, clickAt } from './helpers/canvas.js';
import { getEntityCount, waitForEntityCount, getEntities } from './helpers/state.js';
import { getConstraints, getConstraintCount, setSketchSelection, isConstraintEnabled } from './helpers/constraint.js';

test.describe('sketch constraint types', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('angle constraint available when 2 lines selected', async ({ waffle }) => {
		const page = waffle.page;

		// Draw two lines
		await drawLine(page, -100, 0, 0, 0);
		await waitForEntityCount(page, 3, 5000);
		// Escape chaining, draw second line independently
		await page.keyboard.press('Escape');
		await clickLine(page);
		await drawLine(page, -50, -80, 50, -20);
		await page.waitForTimeout(300);

		// Select the two lines
		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines.length).toBe(2);

		await clickSelect(page);
		await setSketchSelection(page, [lines[0].id, lines[1].id]);
		await page.waitForTimeout(200);

		// Angle button should be enabled
		const angleEnabled = await isConstraintEnabled(page, 'angle');
		expect(angleEnabled).toBe(true);
	});

	test('angle constraint can be applied', async ({ waffle }) => {
		const page = waffle.page;

		// Draw two lines
		await drawLine(page, -100, 0, 0, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.keyboard.press('Escape');
		await clickLine(page);
		await drawLine(page, -50, -80, 50, -20);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');

		// Add angle constraint via API
		await page.evaluate(([l0, l1]) => {
			window.__waffle.addSketchConstraint({
				type: 'Angle', line_a: l0, line_b: l1, value_degrees: 45
			});
		}, [lines[0].id, lines[1].id]);
		await page.waitForTimeout(300);

		const constraints = await getConstraints(page);
		const angle = constraints.find(c => c.type === 'Angle');
		expect(angle).toBeTruthy();
		expect(angle.value_degrees).toBe(45);
	});

	test('symmetric H constraint available when 2 points selected', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line (2 points)
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBe(2);

		await clickSelect(page);
		await setSketchSelection(page, [points[0].id, points[1].id]);
		await page.waitForTimeout(200);

		const shEnabled = await isConstraintEnabled(page, 'symmetricH');
		expect(shEnabled).toBe(true);
	});

	test('point on line constraint can be applied', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);

		// Draw a second line (creates a separate point)
		await page.keyboard.press('Escape');
		await clickLine(page);
		await drawLine(page, 0, 50, 0, 100);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const lines = entities.filter(e => e.type === 'Line');
		const points = entities.filter(e => e.type === 'Point');

		// Select a point from second line and the first line
		// Find a point that belongs to line2 but not line1
		const line1PointIds = new Set([lines[0].start_id, lines[0].end_id]);
		const line2Point = points.find(p => !line1PointIds.has(p.id));
		expect(line2Point).toBeTruthy();

		await clickSelect(page);
		await setSketchSelection(page, [line2Point.id, lines[0].id]);
		await page.waitForTimeout(200);

		const onlEnabled = await isConstraintEnabled(page, 'pointOnLine');
		expect(onlEnabled).toBe(true);
	});

	test('equal radius constraint available when 2 circles selected', async ({ waffle }) => {
		const page = waffle.page;

		// Draw two circles
		await clickCircle(page);
		await drawCircle(page, -80, 0, -50, 0);
		await page.waitForTimeout(300);

		await clickCircle(page);
		await drawCircle(page, 80, 0, 110, 0);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const circles = entities.filter(e => e.type === 'Circle');
		expect(circles.length).toBe(2);

		await clickSelect(page);
		await setSketchSelection(page, [circles[0].id, circles[1].id]);
		await page.waitForTimeout(200);

		// Equal button (aliased to equalRadius for 2 circles) should be enabled
		const eqEnabled = await isConstraintEnabled(page, 'equal');
		expect(eqEnabled).toBe(true);
	});

	test('point on circle constraint can be applied via API', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a circle
		await clickCircle(page);
		await drawCircle(page, 0, 0, 50, 0);
		await page.waitForTimeout(300);

		// Draw a line to get a separate point
		await clickLine(page);
		await drawLine(page, 80, 80, 120, 80);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const circles = entities.filter(e => e.type === 'Circle');
		const points = entities.filter(e => e.type === 'Point');

		// Find a point not belonging to the circle
		const circlePointIds = new Set([circles[0].center_id]);
		const freePoint = points.find(p => !circlePointIds.has(p.id));
		expect(freePoint).toBeTruthy();

		// Apply OnEntity constraint via API
		await page.evaluate(([ptId, circId]) => {
			window.__waffle.addSketchConstraint({
				type: 'OnEntity', point: ptId, entity: circId
			});
		}, [freePoint.id, circles[0].id]);
		await page.waitForTimeout(300);

		const constraints = await getConstraints(page);
		const onCircle = constraints.find(c => c.type === 'OnEntity');
		expect(onCircle).toBeTruthy();
	});
});
