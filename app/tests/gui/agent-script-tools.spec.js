/**
 * The custom-feature-script agent tools (A-M4 of
 * `specs/custom_features_and_modeling_roadmap.md` §A8) through the page's
 * executor against the REAL engine: the agent's authoring loop —
 * `script_run_check` → `script_source_add` → `script_feature_add` →
 * `feature_get` → `script_source_update` — with exact-volume oracles, the
 * rollback of a breaking source update, the document round trip of an
 * embedded Script source, and every call going through one `Tool` send.
 * Refusal codes and delta shapes are pinned in
 * `crates/wasm-bridge/tests/tool_script.rs`.
 */
import { test, expect } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };

const BOX_SCRIPT = `// @feature name="Box" version=1
// @param width: length = 0.02 min=0.001
// @param height: length = 0.01
// @param depth: length = 0.005
// @param plane: plane
// @output body: main
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.width, p.height);
    let r = sk.finish().regions();
    ctx.log("regions: " + r.len());
    ctx.extrude(r[0], #{ depth: p.depth })
}
`;

test.describe('Script agent tools run in the engine (A-M4)', () => {
	test('the authoring loop builds, measures, updates, rolls back and round-trips a script', async ({ waffle }) => {
		test.setTimeout(120000);
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.executeTool === 'function', { timeout: 15000 });

		const result = await page.evaluate(
			async ({ xy, box }) => {
				const api = window.__waffleAgentExecutor;
				const ctx = { agentName: 'script-tools-test', isPaused: () => false, pause: () => {}, isCancelled: () => false };
				const call = (tool, args = {}) => api.executeTool(tool, args, ctx);
				window.__waffle.recordEngineSends(true, { payloads: true });
				const out = {};

				// 1. Check before anything exists; a dry run with the intended args.
				out.check = await call('script_run_check', { text: box, args: { plane: xy } });
				if (out.check.isError) return { failed: 'script_run_check', detail: out.check.structuredContent };
				out.brokenCheck = await call('script_run_check', { text: box.replace('r[0]', 'r[7]'), args: { plane: xy } });

				// 2. Add the source; list it.
				out.added = await call('script_source_add', { text: box });
				if (out.added.isError) return { failed: 'script_source_add', detail: out.added.structuredContent };
				const sourceId = out.added.structuredContent.source_id;
				out.listed = await call('script_source_get');
				out.info = await call('document_info');

				// 3. One node; the exact box.
				out.node = await call('script_feature_add', { source_id: sourceId, args: { plane: xy } });
				if (out.node.isError) return { failed: 'script_feature_add', detail: out.node.structuredContent };
				const featureId = out.node.structuredContent.feature_id;
				out.summary = await call('model_summary');
				out.feature = await call('feature_get', { feature_id: featureId });
				const bodyId = out.summary.structuredContent.bodies[0]?.body_id;
				out.measured = await call('body_measure', { body_id: bodyId });

				// 4. Update the source (a new depth default): the node regenerates.
				out.updated = await call('script_source_update', { source_id: sourceId, text: box.replace('depth: length = 0.005', 'depth: length = 0.007') });
				if (out.updated.isError) return { failed: 'script_source_update', detail: out.updated.structuredContent };
				const after = await call('model_summary');
				out.remeasured = await call('body_measure', { body_id: after.structuredContent.bodies[0]?.body_id });

				// 5. A breaking update is rolled back; the source and the body stand.
				out.broken = await call('script_source_update', { source_id: sourceId, text: box.replace('r[0]', 'r[7]') });
				out.source = await call('script_source_get', { source_id: sourceId });
				const still = await call('model_summary');
				out.stillMeasured = await call('body_measure', { body_id: still.structuredContent.bodies[0]?.body_id });

				// 6. Kept with error: feature_get shows the script's typed error.
				out.kept = await call('script_source_update', { source_id: sourceId, text: box.replace('r[0]', 'r[7]'), on_error: 'keep' });
				out.featureBroken = await call('feature_get', { feature_id: featureId });
				out.restored = await call('script_source_update', { source_id: sourceId, text: box });

				// 7. The document carries the source: save → load → the node builds.
				const json = await window.__waffle.buildDocumentJson();
				const doc = JSON.parse(json);
				out.savedSources = doc.sources ?? [];
				await window.__waffle.loadProject(json);
				const reloaded = await call('model_summary');
				out.reloaded = reloaded.structuredContent;
				out.reloadedMeasured = await call('body_measure', { body_id: reloaded.structuredContent.bodies[0]?.body_id });

				const sends = window.__waffle.getEngineSendLog().filter((s) => s.origin === 'agent');
				window.__waffle.recordEngineSends(false);
				out.toolSends = sends.filter((s) => s.type === 'Tool').map((s) => s.message?.name);
				out.otherAgentSends = sends.filter((s) => s.type !== 'Tool').map((s) => s.type);
				return out;
			},
			{ xy: XY, box: BOX_SCRIPT }
		);

		expect(result.failed, `${result.failed}: ${JSON.stringify(result.detail)}`).toBeUndefined();

		// 1. The check reports the interface and the dry run's children/logs.
		const check = result.check.structuredContent;
		expect(check.ok).toBe(true);
		expect(check.interface.name).toBe('Box');
		expect(check.interface.params.map((p) => [p.name, p.type])).toEqual([
			['width', 'length'],
			['height', 'length'],
			['depth', 'length'],
			['plane', 'plane']
		]);
		expect(check.dry_run).toMatchObject({ ok: true, children: ['sketch', 'extrude'], logs: ['regions: 1'], outputs: ['body'] });
		expect(result.brokenCheck.structuredContent.ok).toBe(true);
		expect(result.brokenCheck.structuredContent.dry_run.ok).toBe(false);
		expect(result.brokenCheck.structuredContent.dry_run.error.stage).toBe('runtime');

		// 2. The source is in the document's sources table as a Script.
		expect(result.added.structuredContent.name).toBe('Box');
		expect(result.listed.structuredContent.scripts.map((s) => s.name)).toEqual(['Box']);
		expect(result.listed.structuredContent.library).toEqual(['gear', 'sprocket']);
		expect(result.info.structuredContent.sources.map((s) => s.kind)).toEqual(['Script']);

		// 3. One Script node named by the script, agent provenance, exact volume.
		expect(result.node.structuredContent.features_added).toHaveLength(1);
		expect(result.node.structuredContent.errors).toEqual([]);
		expect(result.summary.structuredContent.features.map((f) => [f.kind, f.name])).toEqual([['Script', 'Box']]);
		expect(result.summary.structuredContent.features[0].provenance).toEqual({ type: 'Agent', name: 'script-tools-test' });
		expect(result.feature.structuredContent.operation.type).toBe('Script');
		expect(result.feature.structuredContent.error).toBeUndefined();
		expect(result.measured.structuredContent.method).toBe('exact');
		expect(Math.abs(result.measured.structuredContent.volume_m3 - 1e-6)).toBeLessThanOrEqual(1e-15);

		// 4. The regenerated node has the new default depth.
		expect(result.updated.structuredContent.errors).toEqual([]);
		expect(result.updated.structuredContent.interface.params[2].default).toBe(0.007);
		expect(Math.abs(result.remeasured.structuredContent.volume_m3 - 1.4e-6)).toBeLessThanOrEqual(1e-15);

		// 5. The breaking update was rolled back: same text, same body.
		expect(result.broken.isError).toBe(true);
		expect(result.broken.structuredContent.error.code).toBe('FeatureRebuildFailed');
		expect(result.broken.structuredContent.error.details.rolled_back).toBe(true);
		expect(result.broken.structuredContent.error.details.engine_error.kind).toEqual({ type: 'Script', stage: 'runtime' });
		expect(result.source.structuredContent.text).toContain('depth: length = 0.007');
		expect(result.source.structuredContent.features.map((f) => f.name)).toEqual(['Box']);
		expect(Math.abs(result.stillMeasured.structuredContent.volume_m3 - 1.4e-6)).toBeLessThanOrEqual(1e-15);

		// 6. Kept: the node's error is the script's, verbatim.
		expect(result.kept.structuredContent.kept_with_error).toBe(true);
		expect(result.featureBroken.structuredContent.error).toContain('runtime');
		expect(result.restored.structuredContent.errors).toEqual([]);

		// 7. Saved as an embedded Script source; the reloaded document builds it.
		expect(result.savedSources).toHaveLength(1);
		expect(result.savedSources[0].kind).toEqual({ type: 'Script' });
		expect(typeof result.savedSources[0].embed?.blob).toBe('string');
		expect(result.reloaded.features.map((f) => [f.kind, f.name])).toEqual([['Script', 'Box']]);
		expect(result.reloaded.errors).toEqual([]);
		expect(Math.abs(result.reloadedMeasured.structuredContent.volume_m3 - 1e-6)).toBeLessThanOrEqual(1e-15);

		// Every script tool was one `Tool` send to the engine.
		expect(result.toolSends.filter((n) => n.startsWith('script_'))).toEqual([
			'script_run_check',
			'script_run_check',
			'script_source_add',
			'script_source_get',
			'script_feature_add',
			'script_source_update',
			'script_source_update',
			'script_source_get',
			'script_source_update',
			'script_source_update'
		]);
		expect(result.otherAgentSends).toEqual([]);
		expectNoAnyCrash(crashes);
	});
});
