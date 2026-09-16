/**
 * The S3 C4 differential (`specs/waffle_server_mode.md` §2.3): for every tool
 * whose semantics moved into the engine, the document the engine builds must
 * equal the document the page's JS commands built, step for step.
 *
 * The read-only tools of C1–C3 are compared by SHADOWING — running both on the
 * same document and diffing the answers. That cannot work here: a step that
 * changes the document cannot be run twice on it without applying it twice. So
 * each scripted sequence runs on TWO FRESH DOCUMENTS, once with
 * `setEngineTools(false)` (the JS bodies in `commands.js`) and once with it on
 * (the engine's `execute_tool`), and the two are compared canonically —
 * feature UUIDs are minted per run, so they are renamed in structural order.
 *
 * Both arms are checked to have actually taken the path they claim: the engine
 * arm must send `Tool` messages and the JS arm must send none. Two agreeing
 * runs of the same implementation would otherwise prove nothing.
 */
import { test, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
/** 20 × 10 mm rectangle; its loop is lines 5–8. */
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];

/** A reference to an earlier step's answer, resolved in the page. */
const from = (step, field) => ({ $from: step, field });

const extrude = (sketchRef, ids = [5, 6, 7, 8], extra = {}) => ({
	type: 'Extrude',
	params: { sketch_id: sketchRef, profile_index: 0, profile_entity_ids: ids, depth: 0.005, symmetric: false, cut: false, ...extra }
});

/**
 * Each sequence exercises tools that C4 moved. `sketch_create` is still JS in
 * both arms (it moves in C5); it is here to produce something to build on.
 */
const SEQUENCES = [
	['add, rename, suppress', [
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id')) } },
		{ tool: 'feature_rename', args: { feature_id: from(1, 'feature_id'), new_name: 'Base plate' } },
		{ tool: 'feature_suppress', args: { feature_id: from(1, 'feature_id'), suppressed: true } }
	]],
	['reorder and roll back the bar', [
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id')) } },
		{ tool: 'feature_reorder', args: { feature_id: from(1, 'feature_id'), new_position: 0 } },
		{ tool: 'rollback_set', args: { index: 0 } }
	]],
	['delete a depended-on sketch', [
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id')) } },
		{ tool: 'feature_delete', args: { feature_id: from(0, 'feature_id') } }
	]],
	['undo and redo', [
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id')) } },
		{ tool: 'undo', args: {} },
		{ tool: 'redo', args: {} }
	]],
	['a rolled-back step, then a good one', [
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		// Names no loop: the feature builds, fails, and the step is undone.
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id'), [99]) }, expectError: true },
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id')) } }
	]],
	['a kept failing step', [
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id'), [99]), on_error: 'keep' } }
	]],
	['parameters drive a depth', [
		{ tool: 'parameters_set', args: { parameters: [{ name: 'w', expression: '20' }, { name: 'broken', expression: 'nope +' }] } },
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		{ tool: 'feature_add', args: { operation: extrude(from(1, 'feature_id'), [5, 6, 7, 8], { depth_expr: 'w / 4' }) } }
	]],
	['rename a body, then edit the depth', [
		{ tool: 'sketch_create', args: { plane: XY, entities: RECT } },
		{ tool: 'feature_add', args: { operation: extrude(from(0, 'feature_id')) } },
		{ tool: 'body_rename', args: { body_id: from(1, 'bodies_added.0'), new_name: 'Plate' } },
		{ editDepth: { step: 1, depth: 0.01 } }
	]]
];

const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g;

/** Rename every UUID in first-seen order, so two runs become comparable. */
function normalizeUuids(text) {
	const names = new Map();
	return String(text).replace(UUID, (u) => {
		if (!names.has(u)) names.set(u, `uuid-${names.size + 1}`);
		return names.get(u);
	});
}

/**
 * Keys sorted recursively, volatile fields dropped.
 *
 * `id` is NOT dropped: feature, sketch, parameter and document identities are
 * exactly what a divergence would show up in, and renaming the UUIDs in
 * first-seen order over this key-sorted text already makes two runs
 * comparable. Dropping them would only hide a real difference.
 */
function canonical(value) {
	const DROP = new Set(['modified', 'created', 'at', 'preview_mesh']);
	const walk = (v) =>
		Array.isArray(v)
			? v.map(walk)
			: v && typeof v === 'object'
				? Object.fromEntries(
						Object.keys(v)
							.filter((k) => !DROP.has(k))
							.sort()
							.map((k) => [k, walk(v[k])])
					)
				: v;
	return JSON.stringify(walk(value));
}

/**
 * One run, in a form two runs can be compared in: sort, rename, sort.
 *
 * Both halves are needed, because each order alone fails one way:
 *
 * - **Rename before sorting** and the naming depends on the raw key order,
 *   which is implementation-specific — the page builds `{parameters,
 *   ...delta}` so `parameters` comes first, while `serde_json::Map` is a
 *   `BTreeMap` and emits keys alphabetically, so `errors` does. Each arm then
 *   meets a different UUID first and names the same parameter differently.
 * - **Sort before renaming** and any map KEYED by a UUID (`provenance`) sorts
 *   by the raw, freshly minted id, which differs every run.
 *
 * So: sort once to get a structure both arms agree on, rename over that (ids
 * are then reached in the same order — features in tree order, since
 * "features" precedes "provenance"), and sort again so the UUID-keyed maps
 * order by their new names. Key order carries no meaning in JSON, so this
 * compares semantics, not serialization.
 */
function comparable(value) {
	return canonical(JSON.parse(normalizeUuids(canonical(value))));
}

/**
 * Run one sequence on a fresh page and return what it produced.
 * @param {import('@playwright/test').Page} page
 * @param {boolean} useEngine
 * @param {Array<object>} steps
 */
async function runSequence(page, useEngine, steps) {
	await page.goto('/');
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 60000 });
	await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.setEngineTools === 'function', null, { timeout: 15000 });

	return page.evaluate(
		async ({ useEngine, steps }) => {
			const api = window.__waffleAgentExecutor;
			const ctx = {
				agentName: 'rust-authoring-test',
				isPaused: () => false,
				pause: () => {},
				isCancelled: () => false
			};
			api.setEngineTools(useEngine);
			window.__waffle.recordEngineSends(true);

			// `{$from, field}` anywhere in the args means an earlier answer;
			// `field` may be a dotted path with array indices (bodies_added.0).
			const results = [];
			const resolve = (v) => {
				if (Array.isArray(v)) return v.map(resolve);
				if (v && typeof v === 'object') {
					if ('$from' in v) {
						return String(v.field)
							.split('.')
							.reduce((acc, key) => acc?.[key], results[v.$from]);
					}
					return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, resolve(x)]));
				}
				return v;
			};

			for (const step of steps) {
				if (step.editDepth) {
					// feature_edit takes the operation back as feature_get gives it.
					const got = await api.executeTool('feature_get', { feature_id: results[step.editDepth.step].feature_id }, ctx);
					const operation = got.structuredContent.operation;
					operation.params.depth = step.editDepth.depth;
					results.push(
						(await api.executeTool('feature_edit', { feature_id: results[step.editDepth.step].feature_id, operation }, ctx))
							.structuredContent
					);
					continue;
				}
				const result = await api.executeTool(step.tool, resolve(step.args), ctx);
				results.push(result.structuredContent);
			}

			const sends = window.__waffle.getEngineSendLog().filter((s) => s.origin === 'agent');
			return {
				results,
				// `buildDocumentJson` is async: parsing it unawaited yields the
				// string "[object Promise]".
				document: JSON.parse(await window.__waffle.buildDocumentJson()),
				toolSends: sends.filter((s) => s.type === 'Tool').length,
				featureSends: sends.filter((s) => ['AddFeature', 'EditFeature', 'DeleteFeature', 'Undo', 'Redo'].includes(s.type)).length
			};
		},
		{ useEngine, steps }
	);
}

test.describe('Authoring tools: the engine and the page build the same document (S3 C4)', () => {
	test('the twelve authoring tools are the ones routed to the engine', async ({ page }) => {
		await page.goto('/');
		await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.engineTools === 'function', null, { timeout: 30000 });
		const routed = await page.evaluate(() => window.__waffleAgentExecutor.engineTools());
		// Keep in sync with `tools::mutates` and `tools::MIGRATED`.
		expect(routed.sort()).toEqual(
			[
				'body_rename',
				'feature_add',
				'feature_delete',
				'feature_edit',
				'feature_rename',
				'feature_reorder',
				'feature_suppress',
				'import_step',
				'parameters_set',
				'redo',
				'rollback_set',
				'undo'
			].sort()
		);
	});

	for (const [name, steps] of SEQUENCES) {
		test(`C4: ${name}`, async ({ page, context }) => {
			test.setTimeout(180000);
			const crashes = collectCrashErrors(page);

			const viaPage = await runSequence(page, false, steps);

			const enginePage = await context.newPage();
			const engineCrashes = collectCrashErrors(enginePage);
			const viaEngine = await runSequence(enginePage, true, steps);

			// Each arm really took its own path.
			expect(viaPage.toolSends, 'the JS arm must not send Tool').toBe(0);
			expect(viaEngine.toolSends, 'the engine arm must send Tool').toBeGreaterThan(0);
			expect(viaPage.featureSends, 'the JS arm sends the feature messages itself').toBeGreaterThan(0);

			// The answers agree, step for step, once freshly minted ids are renamed.
			expect(comparable(viaEngine.results)).toBe(comparable(viaPage.results));

			// And so do the documents they left behind.
			expect(comparable(viaEngine.document)).toBe(comparable(viaPage.document));

			expectNoAnyCrash(crashes);
			expectNoAnyCrash(engineCrashes);
		});
	}
});
