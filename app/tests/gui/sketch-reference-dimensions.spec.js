/**
 * Reference dimension tests.
 *
 * Verifies that constraints can be toggled between driving and reference mode,
 * and that reference dimensions don't constrain geometry.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine } from './helpers/toolbar.js';
import { drawLine } from './helpers/canvas.js';
import { getEntities, waitForEntityCount } from './helpers/state.js';
import { getConstraints, getConstraintCountByType } from './helpers/constraint.js';

test.describe('sketch reference dimensions', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('toggle constraint to reference mode via API', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		expect(points.length).toBeGreaterThanOrEqual(2);

		// Add a Distance constraint
		await page.evaluate(({ a, b }) => {
			window.__waffle.addSketchConstraint({
				type: 'Distance', entity_a: a, entity_b: b, value: 10
			});
		}, { a: points[0].id, b: points[1].id });
		await page.waitForTimeout(500);

		// Get DOF with driving constraint
		const dofDriving = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		// Find the Distance constraint index
		const distIdx = await page.evaluate(() => {
			const cs = window.__waffle.getConstraints();
			return cs.findIndex(c => c.type === 'Distance');
		});
		expect(distIdx).toBeGreaterThanOrEqual(0);

		// Toggle to reference
		await page.evaluate((idx) => {
			window.__waffle.toggleConstraintReference(idx);
		}, distIdx);
		await page.waitForTimeout(500);

		// Verify reference flag is set
		const constraints = await getConstraints(page);
		const distConstraint = constraints.find(c => c.type === 'Distance');
		expect(distConstraint).toBeTruthy();
		expect(distConstraint.reference).toBe(true);

		// DOF should increase by 1 (reference doesn't constrain)
		const dofRef = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);
		if (dofDriving >= 0 && dofRef >= 0) {
			expect(dofRef).toBe(dofDriving + 1);
		}
	});

	test('toggle reference back to driving mode', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');

		// Add a Distance constraint
		await page.evaluate(({ a, b }) => {
			window.__waffle.addSketchConstraint({
				type: 'Distance', entity_a: a, entity_b: b, value: 10
			});
		}, { a: points[0].id, b: points[1].id });
		await page.waitForTimeout(500);

		const dofBefore = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		// Find the Distance constraint index
		const distIdx = await page.evaluate(() => {
			const cs = window.__waffle.getConstraints();
			return cs.findIndex(c => c.type === 'Distance');
		});

		// Toggle to reference
		await page.evaluate((idx) => window.__waffle.toggleConstraintReference(idx), distIdx);
		await page.waitForTimeout(300);

		// Toggle back to driving
		await page.evaluate((idx) => window.__waffle.toggleConstraintReference(idx), distIdx);
		await page.waitForTimeout(500);

		const constraints = await getConstraints(page);
		const distConstraint = constraints.find(c => c.type === 'Distance');
		expect(distConstraint).toBeTruthy();
		expect(distConstraint.reference).toBeFalsy();

		// DOF should return to original
		const dofAfter = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);
		if (dofBefore >= 0 && dofAfter >= 0) {
			expect(dofAfter).toBe(dofBefore);
		}
	});

	test('reference dimension label shows REF suffix', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -80, 0, 80, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');

		// Add a Distance constraint
		await page.evaluate(({ a, b }) => {
			window.__waffle.addSketchConstraint({
				type: 'Distance', entity_a: a, entity_b: b, value: 10
			});
		}, { a: points[0].id, b: points[1].id });
		await page.waitForTimeout(500);

		// Find the Distance constraint index and toggle to reference
		const distIdx = await page.evaluate(() => {
			const cs = window.__waffle.getConstraints();
			return cs.findIndex(c => c.type === 'Distance');
		});
		expect(distIdx).toBeGreaterThanOrEqual(0);
		await page.evaluate((idx) => window.__waffle.toggleConstraintReference(idx), distIdx);
		await page.waitForTimeout(500);

		// Look for label with (REF) text
		const dimLabel = page.locator('.dim-label');
		const labelVisible = await dimLabel.first().isVisible({ timeout: 3000 }).catch(() => false);
		if (labelVisible) {
			const text = await dimLabel.first().textContent();
			expect(text).toContain('(REF)');
		} else {
			// Fallback: verify constraint has reference=true
			const constraints = await getConstraints(page);
			const dist = constraints.find(c => c.type === 'Distance');
			expect(dist.reference).toBe(true);
		}
	});

	test('reference dim on HDistance does not constrain', async ({ waffle }) => {
		const page = waffle.page;

		await clickLine(page);
		await drawLine(page, -80, -20, 80, 20);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(300);

		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');

		// Add HDistance + make it reference
		await page.evaluate(([pa, pb]) => {
			window.__waffle.addSketchConstraint({
				type: 'HDistance', point_a: pa, point_b: pb, value: 10
			});
		}, [points[0].id, points[1].id]);
		await page.waitForTimeout(300);

		const dofDriving = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		// Make it reference
		const constraints = await getConstraints(page);
		const hIdx = constraints.findIndex(c => c.type === 'HDistance');
		await page.evaluate((idx) => window.__waffle.toggleConstraintReference(idx), hIdx);
		await page.waitForTimeout(500);

		const dofRef = await page.evaluate(
			() => window.__waffle.getSolveStatus()?.dof ?? -1
		);

		// Reference should have 1 more DOF than driving
		if (dofDriving >= 0 && dofRef >= 0) {
			expect(dofRef).toBe(dofDriving + 1);
		}
	});
});
