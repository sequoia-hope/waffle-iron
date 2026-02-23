/**
 * Dimension tool circle resize tests — verifies that applying a radius/diameter
 * constraint via the dimension tool actually changes the circle's radius.
 *
 * Bug: The solver correctly computes the new radius, but the result was silently
 * discarded because the solver result reader only handled x/y axes, not radius.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickCircle, pressKey } from './helpers/toolbar.js';
import { drawCircle } from './helpers/canvas.js';
import {
	getEntities,
	waitForEntityCount,
	getActiveTool,
} from './helpers/state.js';
import {
	getConstraints,
	getConstraintCount,
} from './helpers/constraint.js';

test.describe('dimension tool circle resize', () => {
	test('applying radius constraint changes circle entity radius', async ({ waffle }) => {
		const page = waffle.page;

		// Enter sketch, draw a circle
		await clickSketch(page);
		await clickCircle(page);
		await drawCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		const entitiesBefore = await getEntities(page);
		const circle = entitiesBefore.find(e => e.type === 'Circle');
		expect(circle).toBeTruthy();
		const originalRadius = circle.radius;
		expect(originalRadius).toBeGreaterThan(0);

		// Apply a radius constraint via API (radius = 5.0 → diameter = 10.0)
		const targetRadius = 5.0;
		await page.evaluate(({ circleId, targetRadius }) => {
			window.__waffle.showDimensionPopup({
				entityA: circleId,
				entityB: null,
				sketchX: 1,
				sketchY: 1,
				dimType: 'radius',
				defaultValue: 1.0
			});
			window.__waffle.applyDimensionFromPopup(targetRadius);
		}, { circleId: circle.id, targetRadius });

		// Wait for solver to run and update entities
		await page.waitForTimeout(500);

		// Verify constraint was created
		const constraints = await getConstraints(page);
		const diameterConstraint = constraints.find(c => c.type === 'Diameter');
		expect(diameterConstraint).toBeTruthy();
		expect(diameterConstraint.value).toBe(targetRadius * 2);

		// THE KEY ASSERTION: circle radius should now be updated to the target
		const entitiesAfter = await getEntities(page);
		const circleAfter = entitiesAfter.find(e => e.type === 'Circle');
		expect(circleAfter).toBeTruthy();
		expect(circleAfter.radius).toBeCloseTo(targetRadius, 1);
	});

	test('applying diameter constraint changes circle entity radius', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickCircle(page);
		await drawCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		const entitiesBefore = await getEntities(page);
		const circle = entitiesBefore.find(e => e.type === 'Circle');
		expect(circle).toBeTruthy();

		// Apply Diameter constraint directly (diameter = 8.0 → radius = 4.0)
		const targetDiameter = 8.0;
		await page.evaluate(({ circleId, targetDiameter }) => {
			window.__waffle.addSketchConstraint({ type: 'Diameter', entity: circleId, value: targetDiameter });
		}, { circleId: circle.id, targetDiameter });

		// Wait for solver
		await page.waitForTimeout(500);

		const entitiesAfter = await getEntities(page);
		const circleAfter = entitiesAfter.find(e => e.type === 'Circle');
		expect(circleAfter).toBeTruthy();
		expect(circleAfter.radius).toBeCloseTo(targetDiameter / 2, 1);
	});

	test('circle radius changes visually (different from original)', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickCircle(page);
		// Draw a circle with ~60px radius offset from center
		await drawCircle(page, 0, 0, 60, 0);
		await waitForEntityCount(page, 2, 5000);

		const entitiesBefore = await getEntities(page);
		const circle = entitiesBefore.find(e => e.type === 'Circle');
		expect(circle).toBeTruthy();
		const originalRadius = circle.radius;

		// Apply a very different radius (2.0)
		const targetRadius = 2.0;
		await page.evaluate(({ circleId, targetRadius }) => {
			window.__waffle.showDimensionPopup({
				entityA: circleId,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'radius',
				defaultValue: 1.0
			});
			window.__waffle.applyDimensionFromPopup(targetRadius);
		}, { circleId: circle.id, targetRadius });

		await page.waitForTimeout(500);

		const entitiesAfter = await getEntities(page);
		const circleAfter = entitiesAfter.find(e => e.type === 'Circle');
		expect(circleAfter).toBeTruthy();

		// Radius must have changed from the original
		expect(circleAfter.radius).not.toBeCloseTo(originalRadius, 0);
		expect(circleAfter.radius).toBeCloseTo(targetRadius, 1);
	});

	test('multiple circles can have independent radius constraints', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickCircle(page);

		// Draw first circle
		await drawCircle(page, -80, 0, -40, 0);
		await waitForEntityCount(page, 2, 5000);

		// Draw second circle
		await drawCircle(page, 80, 0, 120, 0);
		await waitForEntityCount(page, 4, 5000);

		const entities = await getEntities(page);
		const circles = entities.filter(e => e.type === 'Circle');
		expect(circles).toHaveLength(2);

		// Apply different radius constraints
		await page.evaluate(({ id1, id2 }) => {
			window.__waffle.addSketchConstraint({ type: 'Diameter', entity: id1, value: 6.0 });
			window.__waffle.addSketchConstraint({ type: 'Diameter', entity: id2, value: 20.0 });
		}, { id1: circles[0].id, id2: circles[1].id });

		await page.waitForTimeout(500);

		const entitiesAfter = await getEntities(page);
		const circlesAfter = entitiesAfter.filter(e => e.type === 'Circle');
		expect(circlesAfter).toHaveLength(2);

		const c1 = circlesAfter.find(e => e.id === circles[0].id);
		const c2 = circlesAfter.find(e => e.id === circles[1].id);
		expect(c1.radius).toBeCloseTo(3.0, 1);
		expect(c2.radius).toBeCloseTo(10.0, 1);
	});
});
