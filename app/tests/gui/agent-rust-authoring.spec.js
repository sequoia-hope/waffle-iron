/**
 * The S3 C4/C4b oracle (`specs/waffle_server_mode.md` §2.3): the twelve
 * authoring tools run in the engine, and they must go on producing exactly
 * what the page's JS commands produced before those bodies were deleted.
 *
 * The read-only tools of C1–C3 are compared by SHADOWING — running both on the
 * same document and diffing the answers. That cannot work here: a step that
 * changes the document cannot be run twice on it without applying it twice. So
 * at C4 each scripted sequence ran on two fresh documents, one arm per
 * implementation, and the two were compared directly. C4b deleted the JS arm,
 * and what it produced is recorded in
 * `fixtures/agent-authoring-goldens.json` — captured from the JS side while it
 * still existed, so the engine is held to an answer that was not derived from
 * it.
 *
 * Feature UUIDs are minted per run, so both sides are canonicalized the same
 * way before comparison (see `comparable`).
 */
import { test, expect } from '@playwright/test';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

/** What the page's JS commands produced, recorded before C4b deleted them. */
const GOLDEN_PATH = resolve(dirname(fileURLToPath(import.meta.url)), 'fixtures/agent-authoring-goldens.json');

/** @type {Record<string, {results: string, document: string}>} */
const GOLDENS = JSON.parse(readFileSync(GOLDEN_PATH, 'utf8'));

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
 * Each sequence exercises tools that C4 moved. `sketch_create` is still JS (it
 * moves in C5); it is here to produce something to build on.
 *
 * A name here is a key in the golden fixture — renaming one silently orphans
 * its recording, which is why every name is checked against the file below.
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
 *   which is implementation-specific — the page built `{parameters,
 *   ...delta}` so `parameters` came first, while `serde_json::Map` is a
 *   `BTreeMap` and emits keys alphabetically, so `errors` does. Each side then
 *   meets a different UUID first and names the same parameter differently.
 * - **Sort before renaming** and any map KEYED by a UUID (`provenance`) sorts
 *   by the raw, freshly minted id, which differs every run.
 *
 * So: sort once to get a structure both sides agree on, rename over that (ids
 * are then reached in the same order — features in tree order, since
 * "features" precedes "provenance"), and sort again so the UUID-keyed maps
 * order by their new names. Key order carries no meaning in JSON, so this
 * compares semantics, not serialization.
 */
function comparable(value) {
	return canonical(JSON.parse(normalizeUuids(canonical(value))));
}

/**
 * Run one sequence on a fresh document and return what it produced.
 * @param {import('@playwright/test').Page} page
 * @param {Array<object>} steps
 */
async function runSequence(page, steps) {
	await page.goto('/');
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 60000 });
	await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.engineTools === 'function', null, { timeout: 15000 });

	return page.evaluate(
		async ({ steps }) => {
			const api = window.__waffleAgentExecutor;
			const ctx = {
				agentName: 'rust-authoring-test',
				isPaused: () => false,
				pause: () => {},
				isCancelled: () => false
			};
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
		{ steps }
	);
}

test.describe('Authoring tools run in the engine and still answer as the page did (S3 C4b)', () => {
	test('every authoring tool is routed to the engine', async ({ page }) => {
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
				'kicad_link',
				'parameters_set',
				'redo',
				'rollback_set',
				'script_feature_add',
				'script_source_add',
				'script_source_update',
				'sketch_create',
				'undo',
				// The tab tools and the assembly edits (2026-09-23).
				'tab_switch',
				'tab_add',
				'tab_move',
				'tab_rename',
				'instance_add',
				'instance_edit',
				'instance_delete',
				'connector_add',
				'connector_edit',
				'connector_delete',
				'mate_add',
				'mate_edit',
				'mate_delete'
			].sort()
		);
	});

	test('every sequence has a recorded golden', () => {
		// A renamed sequence would otherwise just stop being compared.
		expect(SEQUENCES.map(([name]) => name).sort()).toEqual(Object.keys(GOLDENS).sort());
	});

	for (const [name, steps] of SEQUENCES) {
		test(`C4b: ${name}`, async ({ page }) => {
			test.setTimeout(180000);
			const crashes = collectCrashErrors(page);

			const run = await runSequence(page, steps);

			// The engine really served this, and the page authored nothing
			// itself — otherwise a JS fallback could pass the comparison.
			expect(run.toolSends, 'the engine must have served these tools').toBeGreaterThan(0);
			expect(run.featureSends, 'the page must not send feature messages any more').toBe(0);

			// Step for step, and then the document left behind, against what
			// the page's own implementation produced before C4b removed it.
			expect(comparable(run.results)).toBe(GOLDENS[name].results);
			expect(comparable(run.document)).toBe(GOLDENS[name].document);

			expectNoAnyCrash(crashes);
		});
	}
});

/**
 * Re-record the goldens:
 *
 *     CAPTURE_GOLDENS=1 npx playwright test tests/gui/agent-rust-authoring.spec.js -g "golden"
 *
 * The committed file was captured from the PAGE's implementation at C4b, while
 * it still existed, so that the engine is held to an answer it did not author.
 * Re-recording necessarily takes the engine's current output instead, which
 * cannot confirm itself — so treat a regenerated fixture as a change to
 * review, in its diff, against what the behaviour is supposed to be. Do not
 * regenerate to make a red test green.
 */
test.describe('the recorded goldens', () => {
	test.skip(!process.env.CAPTURE_GOLDENS, 'capture run only (CAPTURE_GOLDENS=1)');

	test('re-record the golden answers and documents', async ({ page }) => {
		test.setTimeout(600000);
		// Every sequence runs in the SAME browser context here (one test), so
		// each `goto` would otherwise reopen the previous sequence's work —
		// `restoreOnReload` defaults to 'auto' — and record its leftover
		// errors and shifted ids as if they were this sequence's answer. The
		// comparison runs get a context per test and never see it.
		await page.goto('/');
		await page.waitForFunction(() => window.__waffle?.updateSettings, null, { timeout: 60000 });
		await page.evaluate(() => window.__waffle.updateSettings({ restoreOnReload: 'never' }));
		/** @type {Record<string, {results: string, document: string}>} */
		const goldens = {};
		for (const [name, steps] of SEQUENCES) {
			const run = await runSequence(page, steps);
			goldens[name] = { results: comparable(run.results), document: comparable(run.document) };
		}
		mkdirSync(dirname(GOLDEN_PATH), { recursive: true });
		writeFileSync(GOLDEN_PATH, `${JSON.stringify(goldens, null, '\t')}\n`);
	});
});
