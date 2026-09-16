/**
 * Agent link, Phase 1 authoring (specs/waffle_mcp_server.md §3.2, §3.3, §5): the
 * REAL relay spoken to over MCP stdio, paired with the REAL page by clicking Allow.
 *
 * O1 box volume · O2 exact cylinder · O4 rollback (incl. typed NotSupported) ·
 * O5 keep · O6 one Ctrl+Z per call · O7 no interleaving · O8 busy gates · O9
 * pause/disconnect · O10 provenance · O11 deferred · O12 selection · G2 lock
 * wait · A18 cancel · A7/A13/A14/A16 refusals.
 */
import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { clickFace, getVisibleFaces } from './helpers/geometry.js';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const AGENT = 'agent-authoring-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
/** 20 × 10 mm rectangle (O1); its loop is lines 5–8. */
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];
/** Two driving dimensions that cannot both hold. */
const CONFLICT = [
	{ type: 'HDistance', point_a: 1, point_b: 2, value: 0.02 },
	{ type: 'HDistance', point_a: 1, point_b: 2, value: 0.03 }
];
const FAILED_SOLVE = ['OverConstrained', 'SolveFailed'];
const NIL_UUID = '00000000-0000-4000-8000-000000000000';

function extrude(sketchId, depth, profileEntityIds = [5, 6, 7, 8]) {
	return {
		type: 'Extrude',
		params: { sketch_id: sketchId, profile_index: 0, profile_entity_ids: profileEntityIds, depth, symmetric: false, cut: false }
	};
}

/**
 * The document the save path writes, keys sorted, with volatile fields removed:
 * timestamps (`modified`, provenance `at`) and the derived thumbnail `preview_mesh`.
 * @param {import('@playwright/test').Page} page
 */
async function documentBytes(page) {
	const json = await page.evaluate(() => window.__waffle.buildDocumentJson());
	const DROP = new Set(['modified', 'at', 'preview_mesh']);
	const canon = (v) =>
		Array.isArray(v)
			? v.map(canon)
			: v && typeof v === 'object'
				? Object.fromEntries(
						Object.keys(v)
							.filter((k) => !DROP.has(k))
							.sort()
							.map((k) => [k, canon(v[k])])
					)
				: v;
	return JSON.stringify(canon(JSON.parse(json)));
}

/** @param {import('@playwright/test').Page} page @param {string} text */
function toastCount(page, text) {
	return page.evaluate((t) => window.__waffle.getToasts().filter((x) => JSON.stringify(x).includes(t)).length, text);
}

/**
 * Engine sends made by the agent since recording started. A refused call must
 * make none; the page's own user-origin sends (an opened dialog computing its
 * regions) are not the agent's.
 */
function agentSends(page) {
	return page.evaluate(() => window.__waffle.getEngineSendLog().filter((s) => s.origin === 'agent'));
}

/** @param {any} result */
function ok(result) {
	expect(result.isError, JSON.stringify(result.structuredContent)).toBe(false);
	return result.structuredContent;
}

/** @param {any} result @param {string} code */
function refused(result, code) {
	expect(result.isError, JSON.stringify(result.structuredContent)).toBe(true);
	expect(result.structuredContent.error.code, JSON.stringify(result.structuredContent.error)).toBe(code);
	return result.structuredContent.error;
}

test.describe('Agent link authoring (Phase 1)', () => {
	/** @type {McpRelay} */
	let relay;

	test.beforeEach(async ({ baseURL }) => {
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
		await relay.waitListening();
		await relay.initialize(AGENT);
	});

	test.afterEach(async () => {
		const code = await relay.close();
		for (const line of relay.stdoutLines) expect(JSON.parse(line).jsonrpc).toBe('2.0');
		expect(code).toBe(0);
	});

	test('O1/O10/O6: an exact 20×10×5 mm box, agent provenance, one Ctrl+Z per call', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const docEmpty = await documentBytes(page);

		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));
		expect(sketch.features_added).toEqual([sketch.feature_id]);
		expect(sketch.errors).toEqual([]);
		const loop = sketch.regions.find((r) => [...(r.profile_entity_ids ?? [])].sort((a, b) => a - b).join() === '5,6,7,8');
		expect(loop, JSON.stringify(sketch.regions)).toBeTruthy();
		expect(Math.abs(loop.area_m2 - 2e-4)).toBeLessThan(1e-12);
		const docSketch = await documentBytes(page);

		const box = ok(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005) }));
		expect(box.features_added).toEqual([box.feature_id]);
		expect(box.bodies_added).toHaveLength(1);
		expect(box.errors).toEqual([]);

		const m = ok(await relay.callTool('body_measure', { body_id: box.bodies_added[0] }));
		expect(m.method).toBe('exact');
		expect(Math.abs(m.volume_m3 - 1e-6)).toBeLessThanOrEqual(1e-15);
		const extents = m.bbox_max.map((v, i) => v - m.bbox_min[i]).sort((a, b) => a - b);
		[0.005, 0.01, 0.02].forEach((e, i) => expect(Math.abs(extents[i] - e)).toBeLessThanOrEqual(1e-7));
		expect(Math.abs(m.bbox_min[2])).toBeLessThanOrEqual(1e-7);
		expect(Math.abs(m.bbox_max[2] - 0.005)).toBeLessThanOrEqual(1e-7);
		expect([m.face_count, m.edge_count, m.vertex_count]).toEqual([6, 12, 8]);
		expect(m.closed).toBe(true);
		const bb = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
		for (let a = 0; a < 3; a++) {
			expect(Math.abs(bb.min[a] - m.bbox_min[a])).toBeLessThanOrEqual(1e-6);
			expect(Math.abs(bb.max[a] - m.bbox_max[a])).toBeLessThanOrEqual(1e-6);
		}

		// O10: both features carry the paired agent's name; the tree shows the badge.
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.features.map((f) => f.provenance)).toEqual([
			{ type: 'Agent', name: AGENT },
			{ type: 'Agent', name: AGENT }
		]);
		await expect(page.getByTestId('agent-badge-0')).toBeVisible();
		await expect(page.getByTestId('agent-badge-1')).toBeVisible();

		// O6: a real Ctrl+Z undoes exactly one agent call each.
		await page.keyboard.press('Control+z');
		await expect.poll(() => documentBytes(page), { timeout: 10000 }).toBe(docSketch);
		await page.keyboard.press('Control+z');
		await expect.poll(() => documentBytes(page), { timeout: 10000 }).toBe(docEmpty);

		expectNoAnyCrash(crashes);
	});

	test('O2: an r5 h10 mm cylinder measures exactly', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const sketch = ok(
			await relay.callTool('sketch_create', {
				plane: XY,
				entities: [P(1, 0, 0), { type: 'Circle', id: 2, center_id: 1, radius: 0.005 }]
			})
		);
		expect(sketch.regions.some((r) => JSON.stringify(r.profile_entity_ids) === '[2]'), JSON.stringify(sketch.regions)).toBe(true);
		const cyl = ok(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.01, [2]) }));
		const m = ok(await relay.callTool('body_measure', { body_id: cyl.bodies_added[0] }));
		expect(m.method).toBe('exact');
		expect(Math.abs(m.volume_m3 - Math.PI * 0.005 * 0.005 * 0.01)).toBeLessThanOrEqual(1e-12);
		expectNoAnyCrash(crashes);
	});

	test('O4/A2/A11: failing steps are rolled back byte-exact with one toast', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));

		// A profile that names no loop.
		let before = await documentBytes(page);
		await page.evaluate(() => window.__waffle.dismissAllToasts());
		const noLoop = refused(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005, [99]) }), 'FeatureRebuildFailed');
		expect(noLoop.details.rolled_back).toBe(true);
		expect(noLoop.details.engine_error.kind.type).toBe('ProfileNotFound');
		expect(await documentBytes(page)).toBe(before);
		expect(await toastCount(page, 'Agent step rolled back')).toBe(1);

		// An edit that breaks a downstream feature: the sketch loses its loop.
		const box = ok(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005) }));
		before = await documentBytes(page);
		const op = ok(await relay.callTool('feature_get', { feature_id: sketch.feature_id })).operation;
		op.sketch.entities = op.sketch.entities.filter((e) => e.id !== 7 && e.id !== 8);
		op.sketch.solved_profiles = [];
		const broken = refused(await relay.callTool('feature_edit', { feature_id: sketch.feature_id, operation: op }), 'FeatureRebuildFailed');
		expect(broken.details.feature_id).toBe(box.feature_id);
		expect(await documentBytes(page)).toBe(before);

		// An over-constrained sketch commits nothing.
		const conflict = refused(
			await relay.callTool('sketch_create', { plane: XY, entities: RECT, constraints: CONFLICT }),
			'SketchSolveFailed'
		);
		expect(FAILED_SOLVE).toContain(conflict.details.status);
		expect(await documentBytes(page)).toBe(before);

		expectNoAnyCrash(crashes);
	});

	test('O4/A4: a kernel NotSupported step is rolled back with code NotSupported', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		// A D-shaped sketch: an arc closed by a line, committed with its sampled polygon.
		const d = ok(
			await relay.callTool('sketch_create', {
				plane: XY,
				entities: [P(1, 0, 0), P(2, 0.01, 0), P(3, -0.01, 0), { type: 'Arc', id: 4, center_id: 1, start_id: 2, end_id: 3 }, L(5, 3, 2)]
			})
		);
		const op = ok(await relay.callTool('feature_get', { feature_id: d.feature_id })).operation;
		expect(op.sketch.solved_profiles[0].arc_segments.length).toBeGreaterThan(0);
		// The same sketch without the polygon: kernel-v2 walls an arc profile it cannot chord (typed NotSupported).
		op.sketch.solved_profiles = op.sketch.solved_profiles.map((profile) => ({ ...profile, vertex_ids: [] }));
		const bare = ok(await relay.callTool('feature_add', { operation: op }));

		const before = await documentBytes(page);
		await page.evaluate(() => window.__waffle.dismissAllToasts());
		const wall = refused(
			await relay.callTool('feature_add', {
				operation: { type: 'Extrude', params: { sketch_id: bare.feature_id, profile_index: 0, depth: 0.005, symmetric: false, cut: false } }
			}),
			'NotSupported'
		);
		expect(wall.details.rolled_back).toBe(true);
		expect(wall.details.engine_error.kind.type).toBe('NotSupported');
		expect(wall.message).toContain('arc-segment profile without an authored vertex_ids polygon');
		expect(await documentBytes(page)).toBe(before);
		expect(await toastCount(page, 'Agent step rolled back')).toBe(1);

		expectNoAnyCrash(crashes);
	});

	test('O5/A3/A12: on_error keep leaves the failing step in place', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));

		const kept = ok(
			await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005, [99]), on_error: 'keep' })
		);
		expect(kept.kept_with_error).toBe(true);
		expect(kept.features_added).toEqual([kept.feature_id]);
		expect(kept.errors.map((e) => e.feature_id)).toContain(kept.feature_id);
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.features.map((f) => f.id)).toContain(kept.feature_id);

		const conflicted = ok(
			await relay.callTool('sketch_create', { plane: XY, entities: RECT, constraints: CONFLICT, on_error: 'keep' })
		);
		expect(FAILED_SOLVE).toContain(conflicted.solve_status);
		expect(conflicted.features_added).toEqual([conflicted.feature_id]);

		expectNoAnyCrash(crashes);
	});

	test('O11/A7/A13/A14/A16: refusals reach no engine', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		refused(await relay.callTool('undo'), 'NothingToUndo');
		refused(await relay.callTool('redo'), 'NothingToRedo');

		const documentBefore = await documentBytes(page);
		await page.evaluate(() => window.__waffle.recordEngineSends(true));
		for (const [type, params] of [
			['Fillet', { edges: [], radius: 0.001 }],
			['Chamfer', { edges: [], distance: 0.001 }],
			['Shell', { faces_to_remove: [], thickness: 0.001 }]
		]) {
			const deferred = refused(await relay.callTool('feature_add', { operation: { type, params } }), 'Deferred');
			expect(deferred.details.operation).toBe(type);
		}
		const unknown = refused(await relay.callTool('feature_add', { operation: { type: 'Warp', params: {} } }), 'InvalidOperation');
		expect(unknown.details.schema_path).toBe('/operation/type');
		refused(await relay.callTool('feature_delete', { feature_id: NIL_UUID }), 'FeatureNotFound');
		refused(await relay.callTool('body_measure', { body_id: 'no-such-body' }), 'BodyNotFound');
		const dup = refused(
			await relay.callTool('sketch_create', { plane: XY, entities: [P(1, 0, 0), P(1, 1, 1)] }),
			'InvalidSketch'
		);
		expect(dup.message).toContain('/entities/1/id');
		// S3 C4 moved these refusals INTO the engine, so a refused authoring
		// call now costs exactly one `Tool` message — it cannot be decided
		// without asking the engine. The invariant is unchanged in substance
		// (a refusal does no work and changes nothing), so that is what is
		// asserted: no feature-level message, and a byte-identical document.
		// `body_measure` and `sketch_create` still refuse in the page and send
		// nothing at all.
		expect((await agentSends(page)).filter((s) => s.type !== 'Tool')).toEqual([]);
		expect(await documentBytes(page)).toBe(documentBefore);
		const sendsAfterRefusals = (await agentSends(page)).length;

		// A malformed known operation fails the relay's inputSchema check (§6.1) and never reaches the page.
		const malformed = await relay.request('tools/call', {
			name: 'feature_add',
			arguments: { operation: { type: 'Extrude', params: { sketch_id: NIL_UUID } } }
		});
		expect(malformed.error.code).toBe(-32602);
		expect((await agentSends(page)).length).toBe(sendsAfterRefusals);

		expectNoAnyCrash(crashes);
	});

	test('O8/G3/G4: a busy or paused page refuses edits and sends nothing', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));

		// G3 sketch mode, entered with real clicks: toolbar Sketch, then the Front plane in the tree.
		await page.getByTestId('toolbar-btn-sketch').click();
		await page.getByTestId('origin-plane-front').click();
		await page.waitForFunction(() => window.__waffle.getState().sketchMode.active === true, null, { timeout: 5000 });
		await page.evaluate(() => window.__waffle.recordEngineSends(true));
		const inSketch = refused(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005) }), 'UserBusy');
		expect(inSketch.details.reason).toBe('sketch_mode');
		expect(await agentSends(page)).toEqual([]);
		await expect
			.poll(async () => (await relay.callTool('waffle_status')).structuredContent, { timeout: 5000 })
			.toMatchObject({ state: 'busy', busy_reason: 'sketch_mode' });
		// Queries still answer while the user is busy.
		ok(await relay.callTool('model_summary'));
		await page.evaluate(() => window.__waffle.exitSketch());

		// G3 feature dialog, opened with a real click.
		await page.getByTestId('toolbar-btn-extrude').click();
		await page.waitForFunction(() => window.__waffle.getExtrudeDialogState() != null, null, { timeout: 5000 });
		await page.evaluate(() => window.__waffle.recordEngineSends(true));
		const inDialog = refused(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005) }), 'UserBusy');
		expect(inDialog.details.reason).toBe('feature_dialog');
		expect(await agentSends(page)).toEqual([]);
		await page.keyboard.press('Escape');
		await page.waitForFunction(() => window.__waffle.getExtrudeDialogState() == null, null, { timeout: 5000 });

		// G4 pause from the agent bar.
		await page.getByTestId('agent-pause').click();
		await expect
			.poll(async () => (await relay.callTool('waffle_status')).structuredContent.state, { timeout: 5000 })
			.toBe('paused');
		await page.evaluate(() => window.__waffle.recordEngineSends(true));
		refused(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005) }), 'AgentPaused');
		expect(await agentSends(page)).toEqual([]);
		ok(await relay.callTool('model_summary'));
		await page.getByTestId('agent-resume').click();
		const resumed = ok(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005) }));
		expect(resumed.bodies_added).toHaveLength(1);

		expectNoAnyCrash(crashes);
	});

	test('O12/Q1: the face the user clicks is the ref the agent sees and sketches on', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));
		ok(await relay.callTool('feature_add', { operation: extrude(sketch.feature_id, 0.005) }));
		expect(ok(await relay.callTool('selection_get')).selection).toEqual([]);

		await page.evaluate(() => window.dispatchEvent(new Event('waffle-fit-all')));
		await page.waitForTimeout(500);
		const faces = await getVisibleFaces(page);
		expect(faces.length).toBeGreaterThan(0);
		// Real pointer clicks through the picker until one selects a body face.
		let picked = null;
		for (const face of faces) {
			await clickFace(page, face);
			const refs = await page.evaluate(() => JSON.parse(JSON.stringify(window.__waffle.getSelectedRefs())));
			if (refs.length === 1 && refs[0].kind?.type === 'Face' && refs[0].anchor?.type === 'FeatureOutput') {
				picked = refs[0];
				break;
			}
		}
		expect(picked, 'no body face was pickable').toBeTruthy();

		const sel = ok(await relay.callTool('selection_get'));
		expect(sel.selection).toHaveLength(1);
		expect(sel.selection[0].geom_ref).toEqual(picked);
		expect(sel.selection[0].kind).toBe('Face');
		expect(sel.selection[0].body_id).toBeTruthy();

		const facePlane = await page.evaluate((ref) => window.__waffle.computeFacePlane(ref), picked);
		const onFace = ok(
			await relay.callTool('sketch_create', {
				plane: sel.selection[0].geom_ref,
				entities: [P(1, 0, 0), { type: 'Circle', id: 2, center_id: 1, radius: 0.001 }]
			})
		);
		const normal = ok(await relay.callTool('feature_get', { feature_id: onFace.feature_id })).operation.sketch.plane_normal;
		for (let a = 0; a < 3; a++) expect(Math.abs(normal[a] - facePlane.normal[a])).toBeLessThanOrEqual(1e-9);

		expectNoAnyCrash(crashes);
	});

	test.describe('during a slow agent call', () => {
		/**
		 * A 21-tooth gear sketch whose extrude takes the kernel about two seconds
		 * here (measured 2026-09-14): long enough to act while the call runs.
		 */
		async function gearSketch() {
			const sketch = ok(
				await relay.callTool('sketch_create', {
					plane: XY,
					entities: [{ type: 'Gear', id: 1, params: { toothCount: 21, module: 0.0364 } }]
				})
			);
			return {
				type: 'Extrude',
				params: { sketch_id: sketch.feature_id, profile_index: 0, depth: 0.25, symmetric: false, cut: false }
			};
		}

		/** @param {import('@playwright/test').Page} page */
		async function waitForAgentActivity(page) {
			await page.waitForFunction(() => window.__waffle.getAgentActivity() !== null, null, { timeout: 10000 });
		}

		test('O7/G8: user commands during the call send nothing between the agent sends', async ({ page }) => {
			const crashes = collectCrashErrors(page);
			await pairAgent(page, relay, AGENT);
			const operation = await gearSketch();
			const featuresBefore = (await page.evaluate(() => window.__waffle.getFeatureTree().features.length));

			await page.evaluate(() => window.__waffle.recordEngineSends(true));
			const call = relay.callTool('feature_add', { operation });
			await waitForAgentActivity(page);
			await expect(page.getByTestId('agent-bar-activity')).toContainText('feature_add');
			// Real pointer and keyboard input while the call runs.
			await page.getByTestId('toolbar-btn-extrude').click({ force: true });
			await page.keyboard.press('e');
			await page.keyboard.press('Control+z');
			await page.getByTestId('toolbar-btn-undo').click({ force: true });
			const added = ok(await call);
			await page.waitForFunction(() => window.__waffle.getAgentActivity() === null);
			await page.waitForTimeout(500);

			const log = await page.evaluate(() => window.__waffle.getEngineSendLog());
			const agent = log.map((s, i) => (s.origin === 'agent' ? i : -1)).filter((i) => i >= 0);
			expect(agent.length).toBeGreaterThan(0);
			const between = log.slice(agent[0], agent[agent.length - 1] + 1).filter((s) => s.origin === 'user');
			expect(between).toEqual([]);
			// G8: nothing was queued either — no dialog opened, no undo ran afterwards.
			expect(log.filter((s) => s.origin === 'user' && (s.type === 'Undo' || s.type === 'AddFeature'))).toEqual([]);
			expect(await page.evaluate(() => window.__waffle.getExtrudeDialogState())).toBeNull();
			expect(await page.evaluate(() => window.__waffle.getFeatureTree().features.length)).toBe(featuresBefore + 1);
			expect(added.bodies_added).toHaveLength(1);

			expectNoAnyCrash(crashes);
		});

		test('O9/I12/P12: Pause mid-call lets the call finish; Disconnect reaches the relay within 1 s', async ({ page }) => {
			const crashes = collectCrashErrors(page);
			await pairAgent(page, relay, AGENT);
			const operation = await gearSketch();

			const call = relay.callTool('feature_add', { operation });
			await waitForAgentActivity(page);
			await page.getByTestId('agent-pause').click();
			const finished = ok(await call);
			expect(finished.bodies_added).toHaveLength(1);
			refused(await relay.callTool('feature_add', { operation }), 'AgentPaused');
			await expect
				.poll(async () => (await relay.callTool('waffle_status')).structuredContent.state, { timeout: 5000 })
				.toBe('paused');

			const clickedAt = Date.now();
			await page.getByTestId('agent-disconnect').click();
			await expect
				.poll(async () => (await relay.callTool('waffle_status')).structuredContent.state, { timeout: 1000, intervals: [25] })
				.toBe('unpaired');
			expect(Date.now() - clickedAt).toBeLessThanOrEqual(1000);

			expectNoAnyCrash(crashes);
		});

		test('A18: a cancelled call completes, then its step is undone', async ({ page }) => {
			const crashes = collectCrashErrors(page);
			await pairAgent(page, relay, AGENT);
			const operation = await gearSketch();
			const before = await documentBytes(page);

			const { id, response } = relay.start('tools/call', { name: 'feature_add', arguments: { operation } }, 60000);
			response.catch(() => {});
			await waitForAgentActivity(page);
			relay.cancel(id);
			await page.waitForFunction(() => window.__waffle.getAgentActivity() === null, null, { timeout: 30000 });
			await expect.poll(() => documentBytes(page), { timeout: 10000 }).toBe(before);
			// The session stays usable.
			ok(await relay.callTool('model_summary'));

			expectNoAnyCrash(crashes);
		});
	});

	test('G2: a user rebuild holding the engine longer than 10 s refuses the call as rebuilding', async ({ page }) => {
		test.setTimeout(180000);
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		// Loading a five-extrude gear stack is one ~25 s LoadProject rebuild through the user path.
		const f0064 = readFileSync(new URL('../cases/assay/F0064.waffle', import.meta.url), 'utf8');
		await page.evaluate((json) => {
			void window.__waffle.loadProject(json);
		}, f0064);
		await page.waitForFunction(() => window.__waffle.getEngineLockHolder() === 'user', null, { timeout: 10000 });
		await page.evaluate(() => window.__waffle.recordEngineSends(true));

		const t0 = Date.now();
		const busy = refused(await relay.callTool('feature_rename', { feature_id: NIL_UUID, new_name: 'x' }, 60000), 'UserBusy');
		const waited = Date.now() - t0;
		expect(busy.details.reason).toBe('rebuilding');
		expect(waited).toBeGreaterThanOrEqual(9500);
		expect(await agentSends(page)).toEqual([]);

		await page.waitForFunction(() => (window.__waffle.getFeatureTree()?.features?.length ?? 0) === 10, null, { timeout: 120000 });
		expectNoAnyCrash(crashes);
	});
});
