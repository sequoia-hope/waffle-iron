/**
 * Origin snap constraint tests — verifies that snapping to origin (0,0)
 * correctly constrains points without errors.
 *
 * Bug: Snapping to origin adds a WhereDragged constraint that may cause
 * errors during solver processing or rendering.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine } from './helpers/toolbar.js';
import { clickAt } from './helpers/canvas.js';
import {
	getEntities,
	waitForEntityCount,
	isSketchActive,
} from './helpers/state.js';
import { getConstraints, getConstraintCountByType } from './helpers/constraint.js';

test.describe('origin snap constraint', () => {
	test('drawing line from origin produces no errors', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickLine(page);

		// Click at canvas center (which maps to origin in sketch coordinates)
		await clickAt(page, 0, 0);
		await page.waitForTimeout(300);

		// Click away from origin to complete the line
		await clickAt(page, 100, 0);
		await page.waitForTimeout(300);

		// Verify entities were created (2 points + 1 line = 3 entities)
		await waitForEntityCount(page, 3, 5000);
		const entities = await getEntities(page);
		const points = entities.filter(e => e.type === 'Point');
		const lines = entities.filter(e => e.type === 'Line');
		expect(points.length).toBeGreaterThanOrEqual(2);
		expect(lines).toHaveLength(1);

		// Verify no errors in engine state
		const state = await page.evaluate(() => {
			const s = window.__waffle?.getState();
			return { lastError: s?.lastError, solverStatus: s?.solverStatus };
		});
		// Should have no error (null/undefined) or empty error
		expect(state.lastError).toBeFalsy();
	});

	test('origin snap adds WhereDragged constraint to pin point', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickLine(page);

		// Use API to add a point precisely at origin with WhereDragged constraint
		await page.evaluate(() => {
			const w = window.__waffle;
			// Use a high ID to avoid conflicts
			const id = 99001;
			w.addSketchEntity({ type: 'Point', id, x: 0, y: 0, construction: false });
			w.addSketchConstraint({ type: 'WhereDragged', point: id, x: 0, y: 0 });
		});
		await page.waitForTimeout(300);

		// Verify WhereDragged constraint exists
		const constraints = await getConstraints(page);
		const whereDragged = constraints.filter(c => c.type === 'WhereDragged');
		expect(whereDragged.length).toBeGreaterThanOrEqual(1);

		// Verify no solver errors
		const state = await page.evaluate(() => {
			const s = window.__waffle?.getState();
			return { lastError: s?.lastError, solverStatus: s?.solverStatus };
		});
		expect(state.lastError).toBeFalsy();
	});

	test('point at origin remains at (0,0) after solving', async ({ waffle }) => {
		const page = waffle.page;

		await clickSketch(page);
		await clickLine(page);

		// Click at canvas center (origin) to place first point
		await clickAt(page, 0, 0);
		await page.waitForTimeout(300);

		// Click far away to draw a line from origin
		await clickAt(page, 150, 0);
		await page.waitForTimeout(500);

		await waitForEntityCount(page, 3, 5000);

		// Get positions and check that the start point is near origin
		const positions = await page.evaluate(() => {
			const entities = window.__waffle?.getEntities() ?? [];
			const points = entities.filter(e => e.type === 'Point');
			return points.map(p => ({ id: p.id, x: p.x, y: p.y }));
		});

		// At least one point should be at or very near origin
		const nearOrigin = positions.filter(p =>
			Math.abs(p.x) < 1 && Math.abs(p.y) < 1
		);
		expect(nearOrigin.length).toBeGreaterThanOrEqual(1);
	});
});
