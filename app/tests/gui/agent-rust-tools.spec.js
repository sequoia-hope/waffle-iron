/**
 * The S3 differential (`specs/waffle_server_mode.md` §2.3): for every tool whose
 * semantics have moved into the engine, the page's JS implementation and the
 * engine's Rust one must produce identical `structuredContent`.
 *
 * The executor shadows each migrated call (`setShadow(true)`): it returns the
 * page's answer and records any disagreement. So this spec drives a real
 * document through the real tools and then asserts the mismatch log is empty —
 * and that the comparison actually ran, because an empty log proves nothing on
 * its own (`getShadowRuns`).
 *
 * The model is built through the page's own executor, so the summary under
 * test covers what a real session produces: features of several kinds, an
 * agent-authored provenance, a suppressed feature, a rename, design parameters
 * and a body.
 */
import { test, expect } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
/** 20 × 10 mm rectangle; its loop is lines 5–8. */
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];

const extrude = (sketchId, depth) => ({
	type: 'Extrude',
	params: { sketch_id: sketchId, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth, symmetric: false, cut: false }
});

test.describe('Agent tools implemented in the engine (S3)', () => {
	test('the page and the engine summarize the same model', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.setShadow === 'function', {
			timeout: 15000
		});

		const result = await page.evaluate(
			async ({ xy, rect, extrudeOp }) => {
				const api = window.__waffleAgentExecutor;
				const ctx = {
					agentName: 'rust-tools-test',
					isPaused: () => false,
					pause: () => {},
					isCancelled: () => false
				};
				const call = (tool, args = {}) => api.executeTool(tool, args, ctx);

				api.setShadow(true);
				try {
					// An empty document is the first comparison: every list empty,
					// which is exactly where a null-vs-[] difference would hide.
					const empty = await call('model_summary');

					const sketch = await call('sketch_create', { plane: xy, entities: rect });
					if (sketch.isError) return { failed: 'sketch_create', detail: sketch.structuredContent };
					const solid = await call('feature_add', {
						operation: { ...extrudeOp, params: { ...extrudeOp.params, sketch_id: sketch.structuredContent.feature_id } }
					});
					if (solid.isError) return { failed: 'feature_add', detail: solid.structuredContent };

					// Variety the summary has to report: a rename, a suppressed
					// feature, and parameters (one of which does not evaluate).
					await call('feature_rename', { feature_id: solid.structuredContent.feature_id, new_name: 'Base plate' });
					await call('parameters_set', {
						parameters: [
							{ name: 'width', expression: '20' },
							{ name: 'broken', expression: 'nope +' }
						]
					});
					const built = await call('model_summary');

					await call('feature_suppress', { feature_id: solid.structuredContent.feature_id, suppressed: true });
					const suppressed = await call('model_summary');

					return {
						empty: empty.structuredContent,
						built: built.structuredContent,
						suppressed: suppressed.structuredContent,
						runs: api.getShadowRuns(),
						mismatches: api.getShadowMismatches()
					};
				} finally {
					api.setShadow(false);
				}
			},
			{ xy: XY, rect: RECT, extrudeOp: extrude('placeholder', 0.005) }
		);

		expect(result.failed, `${result.failed}: ${JSON.stringify(result.detail)}`).toBeUndefined();

		// The comparison ran — three `model_summary` calls, each shadowed.
		expect(result.runs).toBe(3);
		expect(result.mismatches).toEqual([]);

		// And the model really was non-trivial, so "no mismatch" is not the
		// agreement of two empty answers.
		expect(result.empty.features).toEqual([]);
		expect(result.built.features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		expect(result.built.features[0].provenance).toEqual({ type: 'Agent', name: 'rust-tools-test' });
		expect(result.built.features[1].name).toBe('Base plate');
		expect(result.built.bodies.length).toBeGreaterThan(0);
		expect(result.built.parameters.map((p) => p.name)).toEqual(['width', 'broken']);
		expect(result.suppressed.features[1].suppressed).toBe(true);

		expectNoAnyCrash(crashes);
	});

	test('a tool that has not migrated is refused by the engine, not answered wrongly', async ({ waffle }) => {
		const page = waffle.page;
		await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.shadowedTools === 'function', {
			timeout: 15000
		});

		const shadowed = await page.evaluate(() => window.__waffleAgentExecutor.shadowedTools());
		// Keep in sync with `tools::MIGRATED`; this is the list the differential covers.
		expect(shadowed).toEqual(['model_summary']);
	});
});
