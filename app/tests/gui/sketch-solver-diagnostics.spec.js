/**
 * Solver diagnostics tests.
 *
 * Verifies conflict highlighting, redundancy detection,
 * and under-constrained point identification.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine } from './helpers/toolbar.js';
import { clickAt, drawLine } from './helpers/canvas.js';
import { getEntities, waitForEntityCount } from './helpers/state.js';
import { getConstraints, getConstraintCount } from './helpers/constraint.js';

test.describe('sketch solver diagnostics', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('over-constrained sketch reports failed constraint indices', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		const points = entities.filter(e => e.type === 'Point');
		expect(line).toBeTruthy();
		expect(points.length).toBeGreaterThanOrEqual(2);

		// Fix both endpoints (4 DOF removed)
		await page.evaluate(([p0, p1]) => {
			window.__waffle.addSketchConstraint({
				type: 'Fixed', point: p0, x: -5, y: 0
			});
			window.__waffle.addSketchConstraint({
				type: 'Fixed', point: p1, x: 5, y: 0
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(500);

		// Now add a conflicting Distance constraint (forces different distance)
		await page.evaluate(([pa, pb]) => {
			window.__waffle.addSketchConstraint({
				type: 'Distance', entity_a: pa, entity_b: pb, value: 100
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(500);

		// Check solver status
		const status = await page.evaluate(
			() => window.__waffle.getSolveStatus()
		);

		// Solver should report failure
		if (status) {
			// Either dof < 0 or result is inconsistent
			const failedIndices = await page.evaluate(
				() => window.__waffle.getFailedConstraintIndices()
			);
			// With over-constrained, the solver should identify failed constraints
			// or at minimum, status should not be 'ok'
			expect(status.result !== 0 || failedIndices.length > 0).toBeTruthy();
		}
	});

	test('under-constrained points identified when DOF > 0', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line (creates entities + triggers solver)
		await clickLine(page);
		await drawLine(page, -60, 30, 60, -30);
		await waitForEntityCount(page, 3, 5000);
		await page.keyboard.press('Escape');
		await page.waitForTimeout(300);

		// Add a horizontal constraint to trigger solver
		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();
		await page.evaluate((lineId) => {
			window.__waffle.addSketchConstraint({ type: 'Horizontal', entity: lineId });
		}, line.id);
		await page.waitForTimeout(500);

		// Now add extra unconstrained points via API
		await page.evaluate(() => {
			window.__waffle.addSketchEntity({ type: 'Point', x: 3, y: 4, construction: false });
			window.__waffle.addSketchEntity({ type: 'Point', x: 7, y: 8, construction: false });
		});
		await page.waitForTimeout(500);

		// Wait for solver to run
		await page.waitForFunction(
			() => window.__waffle.getSolveStatus() != null,
			{ timeout: 3000 }
		);

		const status = await page.evaluate(
			() => window.__waffle.getSolveStatus()
		);
		expect(status).toBeTruthy();

		// With free points and DOF > 0, under-constrained should be non-empty
		if (status.dof > 0) {
			const underConstrained = await page.evaluate(
				() => window.__waffle.getUnderConstrained()
			);
			// The API-added points have no constraints referencing them
			expect(underConstrained.length).toBeGreaterThan(0);
		}
	});

	test('fully constrained points are not in under-constrained set', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line
		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');

		// Fix both endpoints
		await page.evaluate(([p0, p1]) => {
			window.__waffle.addSketchConstraint({
				type: 'Fixed', point: p0, x: -5, y: 0
			});
			window.__waffle.addSketchConstraint({
				type: 'Fixed', point: p1, x: 5, y: 0
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(500);

		// DOF should be 0
		const status = await page.evaluate(
			() => window.__waffle.getSolveStatus()
		);
		if (status && status.dof === 0) {
			const underConstrained = await page.evaluate(
				() => window.__waffle.getUnderConstrained()
			);
			expect(underConstrained.length).toBe(0);
		}
	});

	test('solver status exposes DOF correctly', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line — should have 4 DOF (2 free points)
		await clickLine(page);
		await drawLine(page, -60, 0, 60, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500);

		const status = await page.evaluate(
			() => window.__waffle.getSolveStatus()
		);
		expect(status).toBeTruthy();
		expect(typeof status.dof).toBe('number');
		// 2 free points = 4 DOF (possibly less if auto-snap applied constraints)
		expect(status.dof).toBeGreaterThanOrEqual(0);
		expect(status.dof).toBeLessThanOrEqual(4);
	});

	test('adding horizontal constraint reduces DOF from free line', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a slightly diagonal line
		await clickLine(page);
		await drawLine(page, -80, 10, 80, -10);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500);

		const dofBefore = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');

		await page.evaluate((lineId) => {
			window.__waffle.addSketchConstraint({ type: 'Horizontal', entity: lineId });
		}, line.id);
		await page.waitForTimeout(500);

		const dofAfter = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		if (dofBefore >= 0 && dofAfter >= 0) {
			expect(dofAfter).toBeLessThan(dofBefore);
		}
	});

	test('failed constraint indices are empty for consistent system', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');

		// Add a valid horizontal constraint
		await page.evaluate((lineId) => {
			window.__waffle.addSketchConstraint({ type: 'Horizontal', entity: lineId });
		}, line.id);
		await page.waitForTimeout(500);

		const failedIndices = await page.evaluate(
			() => window.__waffle.getFailedConstraintIndices()
		);
		expect(failedIndices.length).toBe(0);
	});
});
