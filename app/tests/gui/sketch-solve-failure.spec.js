/**
 * Failed-solve inertness, end to end (specs/sketch_drag_stability.md B3/I4 +
 * the store apply-gate). A solve that cannot be satisfied must leave sketch
 * geometry exactly where it was — never hand the UI a diverged/partial
 * iterate — while still surfacing the failure status.
 *
 * Fixture: triangle-inequality-violating distances (10 vs 2+2) — independent
 * rows, unsatisfiable, classifies SolveFailed in the solver.
 * Entities/constraints are created via the __waffle API (test SETUP, not
 * drawing behavior).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const getPositions = (page) =>
	page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));

test('unsatisfiable constraints leave positions untouched and report failure', async ({ waffle }) => {
	const page = waffle.page;
	const crashes = collectCrashErrors(page);
	await clickSketch(page);

	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: 0, y: 0, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: 5, y: 0, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: 2.5, y: 2, construction: false });
	});
	await page.waitForTimeout(200);

	// The first two distances are individually satisfiable — they move points
	// legitimately. Let them settle, THEN snapshot, THEN add the third
	// distance that violates the triangle inequality (10 > 2 + 2).
	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchConstraint({ type: 'Distance', entity_a: 1, entity_b: 2, value: 10 });
		w.addSketchConstraint({ type: 'Distance', entity_a: 2, entity_b: 3, value: 2 });
	});
	await page.waitForTimeout(500);
	const before = await getPositions(page);

	await page.evaluate(() => {
		window.__waffle.addSketchConstraint({ type: 'Distance', entity_a: 1, entity_b: 3, value: 2 });
	});

	// The failure status must surface…
	await expect
		.poll(
			async () => (await page.evaluate(() => window.__waffle.getSolveStatus()))?.status,
			{ timeout: 5000 }
		)
		.toBe('SolveFailed');

	// …and the geometry must not have moved (solver echoes input on
	// SolveFailed; the store's apply-gate refuses the result either way).
	const after = await getPositions(page);
	for (const id of [1, 2, 3]) {
		expect(after[id].x).toBeCloseTo(before[id].x, 9);
		expect(after[id].y).toBeCloseTo(before[id].y, 9);
	}

	expectNoAnyCrash(crashes);
});
