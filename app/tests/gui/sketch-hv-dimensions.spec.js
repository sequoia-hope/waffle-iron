/**
 * H/V Distance dimension constraint tests.
 *
 * Verifies horizontal and vertical distance constraints can be applied
 * via API, affect DOF, and produce correct constraint types.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickSelect } from './helpers/toolbar.js';
import { drawLine } from './helpers/canvas.js';
import { getEntities, waitForEntityCount } from './helpers/state.js';
import { getConstraints, getConstraintCount, getConstraintCountByType, setSketchSelection, isConstraintEnabled, clickConstraintButton } from './helpers/constraint.js';

test.describe('sketch H/V distance dimensions', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('add HDistance constraint via API decreases DOF', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a diagonal line (2 points = 4 DOF)
		await clickLine(page);
		await drawLine(page, -80, -40, 80, 40);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500);

		const dofBefore = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		// Get the two point entities
		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBeGreaterThanOrEqual(2);

		// Add HDistance constraint via API
		await page.evaluate(([pa, pb]) => {
			window.__waffle.addSketchConstraint({
				type: 'HDistance', point_a: pa, point_b: pb, value: 5
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(500);

		// Verify constraint was created
		const hCount = await getConstraintCountByType(page, 'HDistance');
		expect(hCount).toBe(1);

		// DOF should decrease by 1
		const dofAfter = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);
		if (dofBefore >= 0 && dofAfter >= 0) {
			expect(dofAfter).toBe(dofBefore - 1);
		}
	});

	test('add VDistance constraint via API decreases DOF', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a diagonal line
		await clickLine(page);
		await drawLine(page, -80, -40, 80, 40);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500);

		const dofBefore = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBeGreaterThanOrEqual(2);

		// Add VDistance constraint via API
		await page.evaluate(([pa, pb]) => {
			window.__waffle.addSketchConstraint({
				type: 'VDistance', point_a: pa, point_b: pb, value: 3
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(500);

		const vCount = await getConstraintCountByType(page, 'VDistance');
		expect(vCount).toBe(1);

		const dofAfter = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);
		if (dofBefore >= 0 && dofAfter >= 0) {
			expect(dofAfter).toBe(dofBefore - 1);
		}
	});

	test('HDistance and VDistance together remove 2 DOF', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -60, -30, 60, 30);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500);

		const dofBefore = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');

		// Add both H and V distance
		await page.evaluate(([pa, pb]) => {
			window.__waffle.addSketchConstraint({
				type: 'HDistance', point_a: pa, point_b: pb, value: 8
			});
			window.__waffle.addSketchConstraint({
				type: 'VDistance', point_a: pa, point_b: pb, value: 4
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(500);

		const hCount = await getConstraintCountByType(page, 'HDistance');
		const vCount = await getConstraintCountByType(page, 'VDistance');
		expect(hCount).toBe(1);
		expect(vCount).toBe(1);

		const dofAfter = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);
		if (dofBefore >= 0 && dofAfter >= 0) {
			expect(dofAfter).toBe(dofBefore - 2);
		}
	});

	test('HDistance constraint button appears for 2-point selection', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		// Switch to select tool and select both points
		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBeGreaterThanOrEqual(2);

		await clickSelect(page);
		await setSketchSelection(page, [points[0].id, points[1].id]);
		await page.waitForTimeout(300);

		// Check if HDistance button is visible
		const hdBtn = page.locator('[data-testid="toolbar-constraint-hDistance"]');
		const hdVisible = await hdBtn.isVisible().catch(() => false);
		expect(hdVisible).toBe(true);

		const vdBtn = page.locator('[data-testid="toolbar-constraint-vDistance"]');
		const vdVisible = await vdBtn.isVisible().catch(() => false);
		expect(vdVisible).toBe(true);
	});

	test('HDistance constraint preserves value after solver', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -80, -20, 80, 20);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');

		await page.evaluate(([pa, pb]) => {
			window.__waffle.addSketchConstraint({
				type: 'HDistance', point_a: pa, point_b: pb, value: 12
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(500);

		// Verify constraint value stored correctly
		const constraints = await getConstraints(page);
		const hDist = constraints.find(c => c.type === 'HDistance');
		expect(hDist).toBeTruthy();
		expect(hDist.value).toBe(12);
	});
});
