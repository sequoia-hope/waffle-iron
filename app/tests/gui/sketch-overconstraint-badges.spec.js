/**
 * Over-constraint badge index mapping, end to end.
 *
 * Two historical off-by-N bugs stacked here:
 *  1. the solver returned residual ROW indices as `conflicts` (multi-row
 *     constraints like Midpoint own 2 rows, shifting everything after them);
 *  2. the store consumed them as sketchConstraints indices without undoing
 *     the reference-dimension filter that triggerSolve applies.
 *
 * Fixture stresses both: a REFERENCE distance first (excluded from the
 * driving list), then a Midpoint (2 rows), then two contradictory Distance
 * constraints. The failed indices must land exactly on the two Distances.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

test('failed-constraint indices land on the conflicting constraints', async ({ waffle }) => {
	const page = waffle.page;
	const crashes = collectCrashErrors(page);
	await clickSketch(page);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: 0, y: 0, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: 5, y: 0, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: 2, y: 1, construction: false });
		w.addSketchEntity({ type: 'Line', id: 10, start_id: 1, end_id: 2, construction: false });
	});
	await page.waitForTimeout(200);
	await page.evaluate(() => {
		const w = window.__waffle;
		// [0] reference dim — measured, NOT driving (excluded from solve)
		w.addSketchConstraint({ type: 'Distance', entity_a: 1, entity_b: 2, value: 99, reference: true });
		// [1] Midpoint — 2 residual rows
		w.addSketchConstraint({ type: 'Midpoint', point: 3, line: 10 });
		// [2] + [3] contradictory driving distances
		w.addSketchConstraint({ type: 'Distance', entity_a: 1, entity_b: 2, value: 10 });
		w.addSketchConstraint({ type: 'Distance', entity_a: 1, entity_b: 2, value: 20 });
	});

	await expect
		.poll(
			async () => (await page.evaluate(() => window.__waffle.getSolveStatus()))?.status,
			{ timeout: 5000 }
		)
		.toBe('OverConstrained');

	const failed = await page.evaluate(() => [...window.__waffle.getFailedConstraintIndices()]);
	const constraints = await page.evaluate(() => window.__waffle.getConstraints());

	expect(failed.length).toBeGreaterThan(0);
	for (const idx of failed) {
		expect(idx, 'failed index within sketchConstraints').toBeLessThan(constraints.length);
		const c = constraints[idx];
		expect(
			c.type === 'Distance' && !c.reference,
			`failed index ${idx} must be a driving Distance, got ${c.type}${c.reference ? ' (reference)' : ''} — wrong badge would highlight`
		).toBe(true);
	}
	// Both contradictory distances (local indices 2 and 3) are the conflicts.
	expect([...failed].sort()).toEqual([2, 3]);

	expectNoAnyCrash(crashes);
});
