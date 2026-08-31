/**
 * Parameterized designs (design variables) — end-to-end GUI coverage.
 *
 * Covers the three user-facing loops:
 *  1. The Variables panel in the feature tree: add / evaluate / edit /
 *     error-flag / delete rows (SetParameters round trip).
 *  2. An extrude whose depth is an expression over a variable; editing the
 *     variable rebuilds the model with the new depth.
 *  3. A sketch dimension driven by an expression via the dimension popup;
 *     editing the variable re-solves the stored sketch on rebuild.
 *
 * Expression semantics under test: bare numbers are mm (lengths) / degrees
 * (angles); the engine (not the JS) is the evaluator of record.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle } from './helpers/toolbar.js';
import { drawLine, drawRectangle } from './helpers/canvas.js';
import {
	getEntities,
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureTree,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';
import { clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { getConstraints } from './helpers/constraint.js';

/** Add a variable through the panel UI. */
async function addVariable(page, name, expression) {
	await page.locator('[data-testid="variable-add"]').click();
	const nameInput = page.locator('[data-testid="variable-name-input"]');
	await nameInput.waitFor({ state: 'visible', timeout: 3000 });
	await nameInput.fill(name);
	const exprInput = page.locator('[data-testid="variable-expr-input"]');
	await exprInput.fill(expression);
	await exprInput.press('Enter');
	await page.locator(`[data-testid="variable-row-${name}"]`).waitFor({ timeout: 5000 });
}

/** Re-open a variable row and change its expression. */
async function editVariable(page, name, newExpression) {
	await page.locator(`[data-testid="variable-row-${name}"]`).click();
	const exprInput = page.locator('[data-testid="variable-expr-input"]');
	await exprInput.waitFor({ state: 'visible', timeout: 3000 });
	await exprInput.fill(newExpression);
	await exprInput.press('Enter');
	await page.locator(`[data-testid="variable-row-${name}"]`).waitFor({ timeout: 5000 });
}

/** The engine-evaluated parameter table via the test API. */
async function getParams(page) {
	return page.evaluate(() => window.__waffle.getParameters());
}

test.describe('variables panel', () => {
	test('add, evaluate, chain, edit, and delete variables', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Add width = 30 → evaluates to 30 (mm-space).
		await addVariable(page, 'width', '30');
		let params = await getParams(page);
		expect(params.length).toBe(1);
		expect(params[0].name).toBe('width');
		expect(params[0].value).toBe(30);
		expect(params[0].error ?? null).toBeNull();
		await expect(page.locator('[data-testid="variable-value-width"]')).toHaveText('30');

		// A dependent variable: half = width / 2 → 15.
		await addVariable(page, 'half', 'width / 2');
		params = await getParams(page);
		expect(params[1].value).toBe(15);

		// Editing width re-evaluates the dependent.
		await editVariable(page, 'width', '40');
		await expect(page.locator('[data-testid="variable-value-half"]')).toHaveText('20');

		// A broken expression flags the row (and only that row).
		await addVariable(page, 'bad', 'nope + 1');
		params = await getParams(page);
		const bad = params.find((p) => p.name === 'bad');
		expect(bad.error).toContain("unknown variable 'nope'");
		expect(params.find((p) => p.name === 'half').error ?? null).toBeNull();

		// Delete the broken row.
		await page.locator('[data-testid="variable-row-bad"]').hover();
		await page.locator('[data-testid="variable-delete-bad"]').click();
		await expect(page.locator('[data-testid="variable-row-bad"]')).toHaveCount(0);
		params = await getParams(page);
		expect(params.map((p) => p.name)).toEqual(['width', 'half']);

		expectNoAnyCrash(crashes);
	});

	test('variables survive unit-suffix expressions', async ({ waffle }) => {
		const page = waffle.page;
		await addVariable(page, 'hole', '0.5in');
		const params = await getParams(page);
		expect(params[0].value).toBeCloseTo(12.7, 9);
	});
});

test.describe('expression-driven extrude depth', () => {
	test('depth = variable; editing the variable rebuilds the model', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await addVariable(page, 'depth', '12');

		// Rectangle sketch → finish → extrude with an expression depth.
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		await clickExtrude(page);
		const depthInput = page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('depth');
		// The live evaluation hint shows the engine's mm-space result.
		await expect(page.locator('[data-testid="extrude-depth-eval"]')).toHaveText('= 12 mm');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);

		let tree = await getFeatureTree(page);
		let extrude = tree.features.find((f) => f.operation?.type === 'Extrude');
		expect(extrude.operation.params.depth_expr).toBe('depth');
		expect(extrude.operation.params.depth).toBeCloseTo(0.012, 12);

		// Change the variable: the extrude's evaluated depth follows.
		await editVariable(page, 'depth', '24');
		await page.waitForFunction(() => {
			const t = window.__waffle.getFeatureTree();
			const e = t.features.find((f) => f.operation?.type === 'Extrude');
			return e && Math.abs(e.operation.params.depth - 0.024) < 1e-12;
		}, { timeout: 10000 });

		tree = await getFeatureTree(page);
		extrude = tree.features.find((f) => f.operation?.type === 'Extrude');
		expect(extrude.operation.params.depth_expr).toBe('depth');

		expectNoAnyCrash(crashes);
	});

	test('bad depth expression blocks apply with an error toast', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('missing_var');
		const evalHint = page.locator('[data-testid="extrude-depth-eval"]');
		await expect(evalHint).toContainText('unknown variable');
		await page.locator('[data-testid="extrude-apply"]').click();
		// No feature was added; the dialog is still open showing the error.
		await page.waitForTimeout(500);
		const tree = await getFeatureTree(page);
		expect(tree.features.length).toBe(1);
	});
});

test.describe('expression-driven sketch dimension', () => {
	test('dimension popup accepts a variable; rebuild re-solves the sketch', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await addVariable(page, 'len', '25');

		await clickSketch(page);
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 3000);

		const entities = await getEntities(page);
		const line = entities.find((e) => e.type === 'Line');
		expect(line).toBeTruthy();

		// Popup on the line (API trigger, same as dimension-tool.spec.js),
		// then type the VARIABLE NAME instead of a number.
		await page.evaluate((lineId) => {
			window.__waffle.showDimensionPopup({
				entityA: lineId,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'distance',
				defaultValue: 0.2,
			});
		}, line.id);
		const input = page.locator('.dimension-input');
		await input.waitFor({ state: 'visible', timeout: 3000 });
		await input.fill('len');
		await page.keyboard.press('Enter');
		await expect(input).not.toBeVisible();

		// The constraint carries the expression AND the evaluated value
		// (25 mm → 0.025 m internal).
		const constraints = await getConstraints(page);
		const dist = constraints.find((c) => c.type === 'Distance');
		expect(dist).toBeTruthy();
		expect(dist.expression).toBe('len');
		expect(dist.value).toBeCloseTo(0.025, 12);

		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		// Editing the variable re-solves the STORED sketch on rebuild.
		await editVariable(page, 'len', '40');
		await page.waitForFunction(() => {
			const t = window.__waffle.getFeatureTree();
			const sk = t.features.find((f) => f.operation?.type === 'Sketch');
			const c = sk?.operation?.sketch?.constraints?.find((x) => x.type === 'Distance');
			return c && Math.abs(c.value - 0.04) < 1e-12;
		}, { timeout: 10000 });

		const tree = await getFeatureTree(page);
		const sk = tree.features.find((f) => f.operation?.type === 'Sketch');
		const c = sk.operation.sketch.constraints.find((x) => x.type === 'Distance');
		expect(c.expression).toBe('len');

		expectNoAnyCrash(crashes);
	});
});
