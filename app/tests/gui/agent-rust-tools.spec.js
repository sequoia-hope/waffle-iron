/**
 * The read-only agent tools run in the engine (`specs/waffle_server_mode.md`
 * §2.3 S3 C5b, C6): `model_summary`, `feature_get`, `body_measure`,
 * `face_list`, `sketch_regions`, `expression_evaluate`, `export_step` and
 * `export_stl` reach the page as one `Tool` send each, and the page has no JS
 * body left for them (the export pair's `deliver:"download"` half excepted,
 * which `agent-export-import.spec.js` covers through the real relay).
 *
 * Until C5b this spec was the S3 differential — the page ran both
 * implementations and this asserted the mismatch log empty. The JS bodies are
 * gone, so there is nothing to compare against any more; what remains to pin
 * is that the answers are about the real model (not matching refusals) and
 * that every one of them went through the engine, which the engine send log
 * shows: `Tool` sends, and none of the engine messages the JS bodies used to
 * send themselves (`MeasureBody`, `ListFaces`, `ComputeRegions`, …).
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

/**
 * The read-only tools, in the order the engine routes them — which is the
 * order the sequence below calls them, except where a tool needs a fixture the
 * sequence has no reason to build. Those are commented where they sit.
 */
const READ_ONLY = [
	'model_summary',
	'feature_get',
	'body_measure',
	// Q1 of `specs/agent_mechanical_design.md` §4.3. The Rust side pins the
	// numbers (`crates/wasm-bridge/tests/measure_distance.rs`); the sequence
	// below calls it so the page's own routing of it is exercised here too.
	'measure_distance',
	// Q2 and Q3 of `specs/agent_mechanical_design.md` §4.3. Both need a
	// SECOND body to be interesting (an interference wants two operands; a
	// mass answer is only worth asserting against a closed form), and the
	// sequence below builds one plate — so their numbers are pinned in Rust
	// (`crates/wasm-bridge/tests/measure_interference_and_mass.rs`) and they
	// are named here for the routing table.
	'measure_interference',
	'measure_mass',
	'face_list',
	// Q6 of `specs/agent_mechanical_design.md` §4.2/§4.3 (2026-10-03). Called
	// by the sequence below on the plate's edges, so the page's own routing of
	// it is exercised here; the numbers are pinned in
	// `crates/wasm-bridge/tests/tool_entity_list.rs`.
	'entity_list',
	'sketch_regions',
	// N1 of `specs/agent_mechanical_design.md` §5.2 (2026-10-03). Called by
	// the sequence below after a face has been named, so the page's routing of
	// it is exercised here; the answers are pinned in
	// `crates/wasm-bridge/tests/tool_names.rs`.
	'names_list',
	'expression_evaluate',
	'export_step',
	'export_stl',
	// `export_dxf` (2026-10-03, drawings D1a) routes like the other exporters.
	// Not called by the sequence: it needs a drawing view to export. Its
	// answers are pinned in `crates/wasm-bridge/tests/tool_export.rs`.
	'export_dxf',
	'script_run_check',
	'script_source_get'
];

/** What the JS bodies sent to the engine themselves before C5b and C6. */
const FORMER_JS_SENDS = [
	'MeasureBody',
	'ListFaces',
	'ComputeRegions',
	'GenerateGearProfile',
	'EvaluateExpression',
	'ExportStep',
	'ExportStl',
	'ExportBodyStl'
];

test.describe('Read-only agent tools run in the engine (S3 C5b)', () => {
	test('every read-only tool answers about the real model through one Tool send', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.executeTool === 'function', {
			timeout: 15000
		});

		const result = await page.evaluate(
			async ({ xy, rect, extrudeOp, formerJsSends }) => {
				const api = window.__waffleAgentExecutor;
				const ctx = {
					agentName: 'rust-tools-test',
					isPaused: () => false,
					pause: () => {},
					isCancelled: () => false
				};
				const call = (tool, args = {}) => api.executeTool(tool, args, ctx);
				// Record every send with its payload: a `Tool` entry's `message.name`
				// is the tool it carried.
				window.__waffle.recordEngineSends(true, { payloads: true });
				const agentSends = () => window.__waffle.getEngineSendLog().filter((s) => s.origin === 'agent');

				// An empty document first: every list empty, which is exactly
				// where a null-vs-[] difference would hide.
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

				const bodyId = built.structuredContent.bodies[0]?.body_id;
				const feature = await call('feature_get', { feature_id: solid.structuredContent.feature_id });
				const measured = await call('body_measure', { body_id: bodyId });
				// A body against a free point 100 mm straight above the middle
				// of its own bounding box, so the closest point is on the top
				// FACE whichever way the sketch plane's in-plane axes fall
				// (they are not world x and y). A planar face against a point
				// is the `exact` tier, and the answer must be the 0.1 m the
				// probe was placed at.
				const bb = [measured.structuredContent.bbox_min, measured.structuredContent.bbox_max];
				const probe = [(bb[0][0] + bb[1][0]) / 2, (bb[0][1] + bb[1][1]) / 2, bb[1][2] + 0.1];
				const distance = await call('measure_distance', {
					a: { type: 'body', body_id: bodyId },
					b: { type: 'point', point: probe }
				});
				const faces = await call('face_list', { body_id: bodyId });
				// Q6: the same body's EDGES, with their arc lengths. A plate
				// extruded from a rectangle has only straight edges, so every
				// one of them must come back `exact`.
				const entities = await call('entity_list', { body_id: bodyId, kind: 'edge' });
				const regions = await call('sketch_regions', { feature_id: sketch.structuredContent.feature_id });
				// N1: name one of the faces just listed, then read the name
				// table back. `entity_name` is a mutating tool, so the
				// authoring spec pins its routing; it is called here because
				// `names_list` has nothing to report without it.
				const entityName = await call('entity_name', {
					target: { type: 'entity', geom_ref: faces.structuredContent.faces[0].geom_ref },
					name: 'first_face'
				});
				const names = await call('names_list', {});
				const expression = await call('expression_evaluate', { expression: 'width * 2' });
				const step = await call('export_step');
				const stl = await call('export_stl', { body_id: bodyId });
				for (const [name, r] of [
					['feature_get', feature],
					['body_measure', measured],
					['measure_distance', distance],
					['face_list', faces],
					['entity_list', entities],
					['sketch_regions', regions],
					['entity_name', entityName],
					['names_list', names],
					['expression_evaluate', expression],
					['export_step', step],
					['export_stl', stl]
				]) {
					if (r.isError) return { failed: name, detail: r.structuredContent };
				}

				await call('feature_suppress', { feature_id: solid.structuredContent.feature_id, suppressed: true });
				const suppressed = await call('model_summary');

				// A refusal is the engine's too: the page has no body to refuse from.
				const missing = await call('body_measure', { body_id: 'no-such-body' });

				const sends = agentSends();
				window.__waffle.recordEngineSends(false);
				return {
					empty: empty.structuredContent,
					built: built.structuredContent,
					suppressed: suppressed.structuredContent,
					feature: feature.structuredContent,
					measured: measured.structuredContent,
					distance: distance.structuredContent,
					faces: faces.structuredContent,
					entities: entities.structuredContent,
					regions: regions.structuredContent,
					entityName: entityName.structuredContent,
					names: names.structuredContent,
					expression: expression.structuredContent,
					// The whole result: the embedded resource is in `content`, and a
					// download side channel must NOT be (there was none asked for).
					step,
					stl,
					missing,
					toolSends: sends.filter((s) => s.type === 'Tool').map((s) => s.message?.name),
					formerJsSends: sends.filter((s) => formerJsSends.includes(s.type)).map((s) => s.type)
				};
			},
			{ xy: XY, rect: RECT, extrudeOp: extrude('placeholder', 0.005), formerJsSends: FORMER_JS_SENDS }
		);

		expect(result.failed, `${result.failed}: ${JSON.stringify(result.detail)}`).toBeUndefined();

		// Every call above was one `Tool` send — the read-only ones included —
		// and the page sent none of the messages its JS bodies used to.
		expect(result.toolSends).toEqual([
			'model_summary',
			'sketch_create',
			'feature_add',
			'feature_rename',
			'parameters_set',
			'model_summary',
			'feature_get',
			'body_measure',
			'measure_distance',
			'face_list',
			'entity_list',
			'sketch_regions',
			'entity_name',
			'names_list',
			'expression_evaluate',
			'export_step',
			'export_stl',
			'feature_suppress',
			'model_summary',
			'body_measure'
		]);
		expect(result.formerJsSends).toEqual([]);

		// The model really was non-trivial, so the answers are not two empty lists.
		expect(result.empty.features).toEqual([]);
		expect(result.built.features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		expect(result.built.features[0].provenance).toEqual({ type: 'Agent', name: 'rust-tools-test' });
		expect(result.built.features[1].name).toBe('Base plate');
		expect(result.built.bodies.length).toBeGreaterThan(0);
		expect(result.built.parameters.map((p) => p.name)).toEqual(['width', 'broken']);
		expect(result.suppressed.features[1].suppressed).toBe(true);

		// …and each read-only tool answered about that model.
		expect(result.feature.operation.type).toBe('Extrude');
		expect(result.measured.volume_m3).toBeGreaterThan(0);
		// Precision 8, not more: the probe was placed off `body_measure`'s
		// bbox, which is derived from the f32 render mesh, so the REFERENCE
		// carries ~1e-9 of its own. The measurement itself is f64 and exact.
		expect(result.distance.distance_m).toBeCloseTo(0.1, 8);
		expect(result.distance.method).toBe('exact');
		// The point lies on nothing; the body's point lies on a face.
		expect(result.distance.on[1]).toBeNull();
		expect(result.distance.on[0].kind).toEqual({ type: 'Face' });
		expect(result.faces.faces.length).toBeGreaterThan(0);
		// Q6: twelve edges of a box-shaped plate, every one an exact straight
		// line, each with a persistent id and the arc length that goes with it.
		expect(result.entities.kind).toBe('edge');
		expect(result.entities.count).toBe(12);
		expect(result.entities.entities.every((e) => e.length.method === 'exact')).toBe(true);
		expect(result.entities.entities.every((e) => e.length.curve_type === 'line')).toBe(true);
		expect(result.entities.entities.every((e) => typeof e.pid === 'number')).toBe(true);
		expect(result.entities.entities.every((e) => e.geom_ref.selector.type === 'Pid')).toBe(true);
		expect(result.entities.body.principal_axes.length).toBe(3);
		// Nothing was filtered, so nothing was excluded — and the answer says
		// so rather than leaving an agent to guess what an empty arm means.
		expect(result.entities.excluded_unevaluable).toBe(0);
		expect(result.entities.unresolved_names).toEqual([]);
		expect(result.regions.regions.length).toBeGreaterThan(0);
		// N1: the name is stored over the face's persistent id, and the
		// listing has it alongside the body's own display name.
		expect(result.entityName.name).toBe('first_face');
		expect(result.entityName.geom_ref.selector.type).toBe('Pid');
		expect(result.names.names.find((n) => n.name === 'first_face')).toMatchObject({
			resolves: true,
			resolved_by: 'pid',
			body: 'Base plate'
		});
		expect(result.expression.value_mm).toBe(40);

		// The export pair (C6): the file embedded as an MCP resource, its size
		// in the description, and nothing out of band — the relay hands
		// `content`/`structuredContent`/`isError` on unchanged.
		expect(result.step.structuredContent).toMatchObject({ deliver: 'agent', mime_type: 'model/step', warnings: [] });
		const stepResource = result.step.content.find((c) => c.type === 'resource')?.resource;
		expect(stepResource.text.startsWith('ISO-10303-21;')).toBe(true);
		expect(result.step.structuredContent.bytes).toBe(Buffer.byteLength(stepResource.text, 'utf8'));
		expect(result.stl.structuredContent).toMatchObject({ deliver: 'agent', mime_type: 'model/stl', file_name: 'Base_plate.stl' });
		expect(typeof result.stl.content.find((c) => c.type === 'resource')?.resource.blob).toBe('string');
		expect(Object.keys(result.step).sort()).toEqual(['content', 'isError', 'structuredContent']);
		expect(Object.keys(result.stl).sort()).toEqual(['content', 'isError', 'structuredContent']);

		// The refusal came back in the MCP error shape, from the engine.
		expect(result.missing.isError).toBe(true);
		expect(result.missing.structuredContent.error.code).toBe('BodyNotFound');

		expectNoAnyCrash(crashes);
	});

	test('the read-only routing table is exactly what the engine implements', async ({ waffle }) => {
		const page = waffle.page;
		await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.engineQueries === 'function', {
			timeout: 15000
		});

		const routed = await page.evaluate(() => window.__waffleAgentExecutor.engineQueries());
		// Keep in sync with `tools::MIGRATED` minus `tools::mutates`
		// (`agent-rust-authoring.spec.js` pins the mutating half). Two of
		// them the sequence above cannot call, so they are named here
		// instead of in READ_ONLY: `sketch3d_get` (2026-09-25) needs a
		// `Sketch3d` feature, and `assembly_get` (2026-09-23) needs an
		// Assembly tab.
		const expected = [...READ_ONLY];
		expected.splice(expected.indexOf('sketch_regions') + 1, 0, 'sketch3d_get');
		// `entity_meta` (2026-09-26, `specs/kicad_board_link.md` C4) needs a
		// linked KiCad board, so it is named here too.
		expect(routed).toEqual([...expected, 'assembly_get', 'entity_meta']);
	});
});
