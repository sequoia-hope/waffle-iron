/**
 * Pipe dialog — B2 checkpoint 3 of `specs/custom_features_and_modeling_roadmap.md`
 * (`specs/b2_pipe_sweep.md`).
 *
 * The path is picked from an inactive sketch; specs testing the APPLY path
 * set it through the test API as SETUP (`__waffle.setPipePath`, which
 * expands a single pick to its connected chain exactly like a viewport
 * click). Covers: toolbar opens the dialog on the last sketch, Apply is
 * disabled without a path, one pick selects the whole chain, a pipe body is
 * built, wall makes it hollow, a corner path is a loud feature error,
 * double-click edits with the saved values, Cancel/Escape.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch, clickPipe } from './helpers/toolbar.js';
import {
	waitForFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getFeatureTree,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

/**
 * A handlebar path as construction geometry (meters): line (−0.1,0)→(0,0),
 * CCW quarter about (0,0.03) to (0.03,0.03), line to (0.03,0.13). Entity
 * ids 10, 11, 12. `withCorner` adds a perpendicular line 13 at (0,0).
 */
async function createPathSketch(waffle, { withCorner = false } = {}) {
	const page = waffle.page;
	await clickSketch(page);
	await page.evaluate((withCorner) => {
		const w = window.__waffle;
		w.addSketchEntity({ id: 1, type: 'Point', x: -0.1, y: 0, construction: false });
		w.addSketchEntity({ id: 2, type: 'Point', x: 0, y: 0, construction: false });
		w.addSketchEntity({ id: 3, type: 'Point', x: 0, y: 0.03, construction: false });
		w.addSketchEntity({ id: 4, type: 'Point', x: 0.03, y: 0.03, construction: false });
		w.addSketchEntity({ id: 5, type: 'Point', x: 0.03, y: 0.13, construction: false });
		w.addSketchEntity({ id: 10, type: 'Line', start_id: 1, end_id: 2, construction: true });
		w.addSketchEntity({ id: 11, type: 'Arc', center_id: 3, start_id: 2, end_id: 4, construction: true });
		w.addSketchEntity({ id: 12, type: 'Line', start_id: 4, end_id: 5, construction: true });
		if (withCorner) {
			w.addSketchEntity({ id: 6, type: 'Point', x: 0, y: 0.1, construction: false });
			w.addSketchEntity({ id: 13, type: 'Line', start_id: 2, end_id: 6, construction: true });
		}
	}, withCorner);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	const tree = await getFeatureTree(page);
	return tree.features.find((f) => f.operation?.type === 'Sketch').id;
}

async function setPath(page, sketchId, ids, opts = {}) {
	await page.evaluate(([sketchId, ids, opts]) => window.__waffle.setPipePath(sketchId, ids, opts), [sketchId, ids, opts]);
}

async function pathIds(page) {
	return page.evaluate(() => window.__waffle.getPipeDialogState()?.entityIds ?? []);
}

test.describe('pipe dialog', () => {
	test('toolbar opens the dialog; Apply waits for a path; one pick selects the chain', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const sketchId = await createPathSketch(waffle);

		await clickPipe(page);
		const dialog = page.locator('[data-testid="pipe-dialog"]');
		await expect(dialog).toBeVisible();
		await expect(page.locator('[data-testid="pipe-apply"]')).toBeDisabled();
		// Path pick mode is armed on open.
		expect(await page.evaluate(() => window.__waffle.getPipeDialogState() != null)).toBe(true);

		// A single pick brings the whole connected chain.
		await setPath(page, sketchId, [11]);
		expect(await pathIds(page)).toEqual([10, 11, 12]);
		await expect(page.locator('[data-testid="pipe-path-item"]')).toContainText('3 segments');
		await expect(page.locator('[data-testid="pipe-apply"]')).toBeEnabled();

		// Clearing empties the path and disables Apply again.
		await page.locator('[data-testid="pipe-path-clear"]').click();
		expect(await pathIds(page)).toEqual([]);
		await expect(page.locator('[data-testid="pipe-apply"]')).toBeDisabled();

		await page.locator('[data-testid="pipe-cancel"]').click();
		await expect(dialog).toBeHidden();
		expectNoAnyCrash(crashes);
	});

	test('applies a solid pipe along the chain and a hollow one with a wall', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const sketchId = await createPathSketch(waffle);

		await clickPipe(page);
		await setPath(page, sketchId, [10]);
		await page.locator('[data-testid="pipe-radius"]').fill('5');
		await page.locator('[data-testid="pipe-apply"]').click();
		await waitForFeatureCount(page, 2, 15000);
		expect(await hasFeatureOfType(page, 'Pipe')).toBe(true);
		expect(await hasMeshWithGeometry(page)).toBe(true);
		let tree = await getFeatureTree(page);
		let pipe = tree.features.find((f) => f.operation?.type === 'Pipe');
		expect(pipe.operation.params.entity_ids).toEqual([10, 11, 12]);
		expect(pipe.operation.params.radius).toBeCloseTo(0.005, 9);
		expect(pipe.operation.params.inner_radius ?? null).toBeNull();
		const errors = await page.evaluate(() => [...window.__waffle.getFeatureErrors().keys()]);
		expect(errors).toEqual([]);

		// Hollow: a second pipe with a 1 mm wall.
		await clickPipe(page);
		await setPath(page, sketchId, [12]);
		await page.locator('[data-testid="pipe-radius"]').fill('5');
		await page.locator('[data-testid="pipe-wall"]').fill('1');
		await page.locator('[data-testid="pipe-apply"]').click();
		await waitForFeatureCount(page, 3, 15000);
		tree = await getFeatureTree(page);
		const pipes = tree.features.filter((f) => f.operation?.type === 'Pipe');
		expect(pipes.length).toBe(2);
		expect(pipes[1].operation.params.inner_radius).toBeCloseTo(0.004, 9);
		expectNoAnyCrash(crashes);
	});

	test('a wall at or over the radius disables Apply', async ({ waffle }) => {
		const page = waffle.page;
		const sketchId = await createPathSketch(waffle);
		await clickPipe(page);
		await setPath(page, sketchId, [10]);
		await page.locator('[data-testid="pipe-radius"]').fill('5');
		await page.locator('[data-testid="pipe-wall"]').fill('5');
		await expect(page.locator('[data-testid="pipe-wall-error"]')).toBeVisible();
		await expect(page.locator('[data-testid="pipe-apply"]')).toBeDisabled();
		await page.locator('[data-testid="pipe-wall"]').fill('2');
		await expect(page.locator('[data-testid="pipe-apply"]')).toBeEnabled();
		await page.keyboard.press('Escape');
		await expect(page.locator('[data-testid="pipe-dialog"]')).toBeHidden();
	});

	test('a corner path is a loud feature error, never a silent body', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const sketchId = await createPathSketch(waffle, { withCorner: true });
		await clickPipe(page);
		// Two lines meeting at a right angle at point 2 (no chain expansion).
		await setPath(page, sketchId, [10, 13], { expand: false });
		expect(await pathIds(page)).toEqual([10, 13]);
		await page.locator('[data-testid="pipe-radius"]').fill('5');
		await page.locator('[data-testid="pipe-apply"]').click();
		await waitForFeatureCount(page, 2, 15000);
		const tree = await getFeatureTree(page);
		const pipe = tree.features.find((f) => f.operation?.type === 'Pipe');
		const message = await page.evaluate((id) => {
			const e = window.__waffle.getFeatureErrors().get(id);
			return e ? (typeof e === 'string' ? e : e.message ?? JSON.stringify(e)) : null;
		}, pipe.id);
		expect(message).toContain('not tangent');
		expectNoAnyCrash(crashes);
	});

	test('double-click edits the pipe with its saved path and radius', async ({ waffle }) => {
		const page = waffle.page;
		const sketchId = await createPathSketch(waffle);
		await clickPipe(page);
		await setPath(page, sketchId, [10]);
		await page.locator('[data-testid="pipe-radius"]').fill('4');
		await page.locator('[data-testid="pipe-wall"]').fill('1');
		await page.locator('[data-testid="pipe-apply"]').click();
		await waitForFeatureCount(page, 2, 15000);
		const tree = await getFeatureTree(page);
		const pipe = tree.features.find((f) => f.operation?.type === 'Pipe');

		const pipeIndex = tree.features.findIndex((f) => f.id === pipe.id);
		await page.locator(`[data-testid="feature-item-${pipeIndex}"]`).dblclick();
		const dialog = page.locator('[data-testid="pipe-dialog"]');
		await expect(dialog).toBeVisible();
		await expect(dialog).toContainText('Edit Pipe');
		expect(await page.locator('[data-testid="pipe-radius"]').inputValue()).toBe('4');
		expect(await page.locator('[data-testid="pipe-wall"]').inputValue()).toBe('1');
		expect(await pathIds(page)).toEqual([10, 11, 12]);

		await page.locator('[data-testid="pipe-radius"]').fill('6');
		await page.locator('[data-testid="pipe-apply"]').click();
		await expect(dialog).toBeHidden();
		await page.waitForFunction(
			(id) => {
				const t = window.__waffle.getFeatureTree();
				const f = t.features.find((x) => x.id === id);
				return Math.abs(f.operation.params.radius - 0.006) < 1e-9;
			},
			pipe.id,
			{ timeout: 15000 }
		);
		await waitForFeatureCount(page, 2, 5000);
	});
});
