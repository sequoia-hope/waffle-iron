/**
 * P3's `*_expr` sidecars across a UI round trip — the silent-data-loss class.
 *
 * P3 gave ten more numeric fields an expression twin. A field whose dialog has
 * no input for its twin has two ways to be silently wrong, and this spec pins
 * both directions for every such field the UI can reach:
 *
 *  - DROPPED: the dialog rebuilds its whole params object from the evaluated
 *    numbers, so an apply that changed nothing sends the number as a literal
 *    and the driver is gone. The user sees correct geometry and a parameter
 *    that has stopped driving it. This is the defect the extrude's second Blind
 *    depth had (`parameterized-designs.spec.js` covers that one, which got a
 *    real input).
 *  - STUCK: the sidecar survives an apply that DID replace the value it drives,
 *    so the next rebuild re-evaluates it over the typed number and overwrites
 *    it. The edit reverts with nothing said.
 *
 * The rule under test, in both the dialogs and the property panel: a value the
 * apply did not change keeps its driver, and a value it replaced loses it.
 *
 * Sidecars are set here through `feature_edit` (the engine's own tool, as an
 * agent would), because no dialog has an input for them — which is exactly why
 * the round trip is the thing worth pinning.
 */
import fs from 'fs';
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	getFeatureTree,
	waitForEntityCount,
	waitForFeatureCount,
} from './helpers/state.js';

const CUBE_STEP = fs.readFileSync(new URL('./fixtures/cube.step', import.meta.url), 'utf8');

/**
 * A 20 × 20 mm rectangle whose every SKETCH-LOCAL coordinate is ≥ 10 mm, so
 * the profile clears a world axis near the origin whichever way the engine's
 * derived in-plane frame maps local x and y (for the XY plane it maps local x
 * to world −Y and local y to world +X, which is not worth depending on).
 */
const RECT_OFF_AXIS = [
	{ type: 'Point', id: 1, x: 0.02, y: 0.02 },
	{ type: 'Point', id: 2, x: 0.04, y: 0.02 },
	{ type: 'Point', id: 3, x: 0.04, y: 0.04 },
	{ type: 'Point', id: 4, x: 0.02, y: 0.04 },
	{ type: 'Line', id: 5, start_id: 1, end_id: 2 },
	{ type: 'Line', id: 6, start_id: 2, end_id: 3 },
	{ type: 'Line', id: 7, start_id: 3, end_id: 4 },
	{ type: 'Line', id: 8, start_id: 4, end_id: 1 },
];

/** Run one engine tool the way the agent link does. */
async function runTool(page, name, args) {
	await page.waitForFunction(
		() => typeof window.__waffleAgentExecutor?.executeTool === 'function',
		null,
		{ timeout: 30000 }
	);
	const result = await page.evaluate(
		async ({ name, args }) => {
			const ctx = {
				agentName: 'sidecar-carry-test',
				isPaused: () => false,
				pause: () => {},
				isCancelled: () => false,
			};
			const r = await window.__waffleAgentExecutor.executeTool(name, args, ctx);
			return { isError: r.isError ?? false, content: r.structuredContent };
		},
		{ name, args }
	);
	expect(result.isError, `${name} failed: ${JSON.stringify(result.content)}`).toBeFalsy();
	return result.content;
}

/** Merge `patch` into a feature's params through `feature_edit`. */
async function patchParams(page, featureId, patch) {
	const got = await runTool(page, 'feature_get', { feature_id: featureId });
	const operation = got.operation;
	Object.assign(operation.params, patch);
	await runTool(page, 'feature_edit', { feature_id: featureId, operation });
}

async function params(page, featureId) {
	const tree = await getFeatureTree(page);
	return tree.features.find((f) => f.id === featureId)?.operation?.params;
}

/** A variable in the tab's table, through the engine. */
async function setVar(page, name, expression) {
	await runTool(page, 'parameters_set', {
		merge: true,
		parameters: [{ name, expression }],
	});
}

/** A finished rectangle sketch, drawn with real pointer events. */
async function rectangleSketch(page, count) {
	await clickSketch(page);
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, count, 10000);
}

test.describe('a dialog with no input for a sidecar carries it', () => {
	test('a revolve keeps axis_origin_expr across an untouched apply, and drops it when the axis moves', async ({
		waffle,
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await setVar(page, 'lift', '4');
		// The profile is authored (test SETUP) so the axis can be placed a
		// known distance off it: a revolve axis must lie IN the sketch plane,
		// so the driven component is x and z stays 0.
		const sketch = await runTool(page, 'sketch_create', {
			plane: { origin: [0, 0, 0], normal: [0, 0, 1] },
			entities: RECT_OFF_AXIS,
		});
		const sketchId = sketch.feature_id;
		await waitForFeatureCount(page, 1, 10000);

		// A revolve about Y, its axis origin x driven by `lift`. The dialog
		// shows the axis as a PICK and has no expression input for it.
		const added = await runTool(page, 'feature_add', {
			operation: {
				type: 'Revolve',
				params: {
					sketch_id: sketchId,
					profile_index: 0,
					profile_entity_ids: [5, 6, 7, 8],
					axis_origin: [0.004, 0, 0],
					axis_direction: [0, 1, 0],
					angle: 360,
					axis_origin_expr: ['lift', null, null],
				},
			},
		});
		const revolveId = added.feature_id;
		expect((await params(page, revolveId)).axis_origin_expr).toEqual(['lift', null, null]);

		// Open the feature for edit and Apply without touching anything.
		await page.evaluate((id) => window.__waffle.showEditFeatureDialog(id), revolveId);
		await page.locator('[data-testid="revolve-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
		await page.locator('[data-testid="revolve-apply"]').click();
		await page.locator('[data-testid="revolve-dialog"]').waitFor({ state: 'hidden', timeout: 15000 });

		let p = await params(page, revolveId);
		expect(p.axis_origin_expr, 'an untouched apply keeps the driver').toEqual(['lift', null, null]);
		expect(p.axis_origin[0]).toBeCloseTo(0.004, 12);

		// The driver still drives: move the variable, the axis follows.
		await setVar(page, 'lift', '9');
		await page.waitForFunction(
			(id) => {
				const f = window.__waffle.getFeatureTree().features.find((x) => x.id === id);
				return f && Math.abs(f.operation.params.axis_origin[0] - 0.009) < 1e-12;
			},
			revolveId,
			{ timeout: 10000 }
		);

		// Now RE-PICK the axis: the value this apply sends replaces the driven
		// one, so the driver goes rather than overwriting it at the next
		// rebuild.
		await page.evaluate((id) => window.__waffle.showEditFeatureDialog(id), revolveId);
		await page.locator('[data-testid="revolve-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
		await page.evaluate(() =>
			window.__waffle.setRevolveAxis([0.002, 0, 0], [0, 1, 0], 'Moved axis')
		);
		await page.locator('[data-testid="revolve-apply"]').click();
		await page.locator('[data-testid="revolve-dialog"]').waitFor({ state: 'hidden', timeout: 15000 });

		p = await params(page, revolveId);
		expect(p.axis_origin[0]).toBeCloseTo(0.002, 12);
		expect(p.axis_origin_expr ?? null, 'a re-picked axis detaches the driver').toBeNull();

		expectNoAnyCrash(crashes);
	});

	test('a mate connector keeps rotation_expr across an untouched apply, and drops it on a rotation edit', async ({
		waffle,
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await setVar(page, 'twist', '15');
		await rectangleSketch(page, 1);
		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('10');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 15000);

		// A connector on the part origin, then a rotation driver on it.
		await page.evaluate(() => window.__waffle.showMateConnectorDialog(null));
		const connectorId = await page.evaluate(() =>
			window.__waffle.applyMateConnector({ name: 'Boss', rotationDeg: 15 })
		);
		expect(connectorId).toBeTruthy();
		await patchParams(page, connectorId, { rotation_expr: 'twist' });
		expect((await params(page, connectorId)).rotation_expr).toBe('twist');

		// Re-open and apply the dialog's own state back, unchanged.
		await page.evaluate((id) => window.__waffle.showMateConnectorDialog(id), connectorId);
		await page.evaluate(async () => {
			const s = window.__waffle.getMateConnectorDialogState();
			await window.__waffle.applyMateConnector(s);
		});
		await page.waitForTimeout(500);
		expect(
			(await params(page, connectorId)).rotation_expr,
			'an untouched apply keeps the driver'
		).toBe('twist');

		// A changed rotation detaches it.
		await page.evaluate((id) => window.__waffle.showMateConnectorDialog(id), connectorId);
		await page.evaluate(async () => {
			const s = window.__waffle.getMateConnectorDialogState();
			await window.__waffle.applyMateConnector({ ...s, rotationDeg: 40 });
		});
		await page.waitForTimeout(500);
		const p = await params(page, connectorId);
		expect(p.rotation_deg).toBeCloseTo(40, 9);
		expect(p.rotation_expr ?? null, 'a changed rotation detaches the driver').toBeNull();

		expectNoAnyCrash(crashes);
	});

	test('an imported body keeps an untouched placement sidecar and drops a replaced one', async ({
		waffle,
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await setVar(page, 'rise', '6');
		const ok = await page.evaluate(
			(text) => window.__waffle.importStepFromText('cube.step', text),
			CUBE_STEP
		);
		expect(ok).toBe(true);
		await waitForFeatureCount(page, 1, 15000);
		await page.locator('[data-testid="import-step-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
		await page.locator('[data-testid="import-apply"]').click();
		await page.locator('[data-testid="import-step-dialog"]').waitFor({ state: 'hidden', timeout: 15000 });

		const tree = await getFeatureTree(page);
		const importId = tree.features.find((f) => f.operation?.type === 'ImportedBody').id;
		// z translation driven, x and y plain — the per-component rule. An
		// ImportedBody refuses `feature_edit` by name (`UseImportTool`), so the
		// sidecar goes in through the DOCUMENT and the file is reloaded, which
		// is also the path a saved parameterised import arrives by.
		await page.evaluate(async (id) => {
			const doc = JSON.parse(await window.__waffle.buildDocumentJson());
			// A tab's content is its serde-tagged `kind`: {type: "Part",
			// features: <FeatureTree>}, so the feature LIST is one level deeper.
			for (const tab of doc.tabs ?? []) {
				const list = tab.kind?.features?.features ?? tab.kind?.features;
				for (const f of Array.isArray(list) ? list : []) {
					if (f.id !== id) continue;
					f.operation.params.translation_m = [0, 0, 0.006];
					f.operation.params.translation_m_expr = [null, null, 'rise'];
				}
			}
			await window.__waffle.loadProject(JSON.stringify(doc));
		}, importId);
		await page.waitForFunction(
			(id) => {
				const f = window.__waffle.getFeatureTree()?.features?.find((x) => x.id === id);
				return !!f?.operation?.params?.translation_m_expr;
			},
			importId,
			{ timeout: 15000 }
		);
		expect((await params(page, importId)).translation_m_expr).toEqual([null, null, 'rise']);

		// An apply that moves only X keeps z's driver and leaves z alone.
		await page.evaluate(
			(id) =>
				window.__waffle.applyImportPlacement(id, {
					translation_m: [0.01, 0, 0.006],
					rotation_deg: [0, 0, 0],
					scale: 1.0,
				}),
			importId
		);
		await page.waitForTimeout(500);
		let p = await params(page, importId);
		expect(p.translation_m_expr, 'an untouched component keeps its driver').toEqual([
			null,
			null,
			'rise',
		]);
		expect(p.translation_m[0]).toBeCloseTo(0.01, 12);

		// An apply that REPLACES z detaches z's driver, so the typed value is
		// not overwritten at the next rebuild.
		await page.evaluate(
			(id) =>
				window.__waffle.applyImportPlacement(id, {
					translation_m: [0.01, 0, 0.025],
					rotation_deg: [0, 0, 0],
					scale: 1.0,
				}),
			importId
		);
		await page.waitForTimeout(500);
		p = await params(page, importId);
		expect(p.translation_m_expr ?? null, 'a replaced component detaches').toBeNull();
		expect(p.translation_m[2]).toBeCloseTo(0.025, 12);

		// And it STAYS: a rebuild does not re-evaluate a detached driver.
		await setVar(page, 'rise', '30');
		await page.waitForTimeout(800);
		p = await params(page, importId);
		expect(p.translation_m[2]).toBeCloseTo(0.025, 12);

		expectNoAnyCrash(crashes);
	});
});

test.describe('the property panel detaches every twinned number', () => {
	test('a pattern count typed in the panel detaches count_expr and holds', async ({ waffle }) => {
		// The panel knew `depth_expr` and `angle_expr` only, so a count, a
		// spacing, a pattern angle and a pipe's two radii kept their driver and
		// the typed number was overwritten at the next rebuild.
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await setVar(page, 'rows', '3');
		await rectangleSketch(page, 1);
		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('10');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 15000);

		const added = await runTool(page, 'feature_add', {
			operation: {
				type: 'PatternLinear',
				params: {
					seeds: { type: 'All' },
					direction: {
						method: 'explicit',
						origin: [0, 0, 0],
						direction: [1, 0, 0],
					},
					count: 3,
					count_expr: 'rows',
					spacing: 0.25,
				},
			},
		});
		const patternId = added.feature_id;
		expect((await params(page, patternId)).count).toBe(3);

		// Select the pattern so the property panel shows it. The tree's testid
		// is the row INDEX, and the pattern is the third feature.
		const index = (await getFeatureTree(page)).features.findIndex((f) => f.id === patternId);
		await page.locator(`[data-testid="feature-item-${index}"]`).click();
		await expect(page.locator('[data-testid="prop-feature-type"]')).toHaveText('PatternLinear');

		// Type a literal count. The debounce in the panel is 300 ms.
		const countInput = page.locator('[data-testid="prop-input-params.count"]');
		await expect(countInput).toHaveValue('3');
		await countInput.fill('5');
		await countInput.blur();
		await page.waitForFunction(
			(id) => {
				const f = window.__waffle.getFeatureTree().features.find((x) => x.id === id);
				return f && f.operation.params.count === 5;
			},
			patternId,
			{ timeout: 10000 }
		);
		expect(
			(await params(page, patternId)).count_expr ?? null,
			'the typed count detached the driver'
		).toBeNull();

		// And the typed count HOLDS: moving the old variable does nothing.
		await setVar(page, 'rows', '8');
		await page.waitForTimeout(800);
		expect((await params(page, patternId)).count).toBe(5);

		expectNoAnyCrash(crashes);
	});
});
