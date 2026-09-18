/**
 * Agent link assembly tools (specs/waffle_mcp_server.md §2.5 Assemblies):
 * `assembly_get`, `instance_*`, `connector_*`, `mate_*` and `tab_switch` onto an
 * Assembly tab — the REAL relay over MCP stdio, paired with the REAL page.
 *
 * The oracle is geometric: a block mated (Fastened, flipped) bottom-face to
 * top-face onto a base plate must end up with its bottom-face centroid exactly
 * on the plate's top-face centroid, whatever in-plane turn the solver chose.
 */
import { test, expect } from '@playwright/test';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const AGENT = 'agent-assembly-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
/** An axis-aligned rectangle from the origin. */
const RECT = (w, h) => [P(1, 0, 0), P(2, w, 0), P(3, w, h), P(4, 0, h), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];
const EPS = 1e-9;

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

/** @param {number[]} a @param {number[]} b */
function expectClose(a, b, eps = EPS) {
	expect(a).toHaveLength(b.length);
	for (let i = 0; i < a.length; i++) expect(Math.abs(a[i] - b[i]), `${JSON.stringify(a)} vs ${JSON.stringify(b)}`).toBeLessThanOrEqual(eps);
}

/** `p' = R p + t` for the document Transform. */
function apply(transform, p) {
	const [x, y, z, w] = transform.rotation_quat;
	const t = transform.translation_m;
	// q * (p, 0) * conj(q)
	const ix = w * p[0] + y * p[2] - z * p[1];
	const iy = w * p[1] + z * p[0] - x * p[2];
	const iz = w * p[2] + x * p[1] - y * p[0];
	const iw = -x * p[0] - y * p[1] - z * p[2];
	return [
		ix * w + iw * -x + iy * -z - iz * -y + t[0],
		iy * w + iw * -y + iz * -x - ix * -z + t[1],
		iz * w + iw * -z + ix * -y - iy * -x + t[2]
	];
}

test.describe('Agent link assemblies', () => {
	/** @type {McpRelay} */
	let relay;

	test.beforeEach(async ({ baseURL }) => {
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
		await relay.waitListening();
		await relay.initialize(AGENT);
	});

	test.afterEach(async () => {
		expect(await relay.close()).toBe(0);
	});

	/**
	 * A `w`×`h`×`d` block on the active Part tab: its top and bottom face refs and
	 * the centre of each, in the part's coordinates (from the measured bbox — the
	 * sketch axes of an origin+normal plane are not the world's x and y).
	 */
	async function block(w, h, d) {
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT(w, h) }));
		ok(
			await relay.callTool('feature_add', {
				operation: {
					type: 'Extrude',
					params: { sketch_id: sketch.feature_id, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth: d, symmetric: false, cut: false }
				}
			})
		);
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.bodies).toHaveLength(1);
		const m = ok(await relay.callTool('body_measure', { body_id: summary.bodies[0].body_id }));
		const cx = (m.bbox_min[0] + m.bbox_max[0]) / 2;
		const cy = (m.bbox_min[1] + m.bbox_max[1]) / 2;
		expect(Math.abs(m.bbox_max[2] - m.bbox_min[2] - d)).toBeLessThanOrEqual(1e-9);
		const faces = ok(await relay.callTool('face_list', { body_id: summary.bodies[0].body_id })).faces;
		expect(faces).toHaveLength(6);
		const withNormal = (n) => {
			const f = faces.find((x) => x.signature.normal && x.signature.normal.every((c, i) => Math.abs(c - n[i]) < 1e-9));
			expect(f, `no face with normal ${n}`).toBeTruthy();
			return f.geom_ref;
		};
		return {
			top: withNormal([0, 0, 1]),
			bottom: withNormal([0, 0, -1]),
			topCentre: [cx, cy, m.bbox_max[2]],
			bottomCentre: [cx, cy, m.bbox_min[2]]
		};
	}

	test('parts placed and mated through the tools land where the solver puts them, and survive a save', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const doc = ok(await relay.callTool('document_new', { name: 'Stack' }));
		const baseTab = doc.active_tab;
		ok(await relay.callTool('tab_rename', { tab_id: baseTab, name: 'Base' }));
		const base = await block(0.02, 0.01, 0.005);

		// Assembly tools need an Assembly tab (the inverse of G7).
		refused(await relay.callTool('assembly_get'), 'TabKindNotSupported');
		expect(refused(await relay.callTool('instance_add', { tab_id: baseTab }), 'TabKindNotSupported').details.kind).toBe('Part');

		const blockTab = ok(await relay.callTool('tab_add', { name: 'Block' })).tab_id;
		const blk = await block(0.01, 0.01, 0.005);

		const asm = ok(await relay.callTool('tab_add', { kind: 'Assembly', name: 'Stack' }));
		expect(asm.active_tab).toBe(asm.tab_id);
		let state = ok(await relay.callTool('assembly_get'));
		expect(state).toMatchObject({ tab_id: asm.tab_id, name: 'Stack', instances: [], connectors: [], mates: [], errors: [] });
		// The parts an instance can be made of: both Part tabs, never the open assembly.
		expect(state.available_parts.map((p) => [p.name, p.kind, p.source_id])).toEqual([['Base', 'Part', null], ['Block', 'Part', null]]);
		refused(await relay.callTool('instance_add', { tab_id: asm.tab_id }), 'TabNotFound');

		// Instances: the base grounded at the origin, the block off to the side.
		const baseInst = ok(await relay.callTool('instance_add', { tab_id: baseTab, fixed: true }));
		expect(baseInst.instances.map((i) => [i.name, i.part_name, i.fixed])).toEqual([['Base 1', 'Base', true]]);
		expectClose(baseInst.instances[0].placement.translation_m, [0, 0, 0]);
		const blockInst = ok(
			await relay.callTool('instance_add', {
				tab_id: blockTab,
				name: 'Lid',
				transform: { translation_m: [0.05, 0, 0], rotation_euler_deg: [0, 0, 90] }
			})
		);
		expect(blockInst.instances.map((i) => i.name)).toEqual(['Base 1', 'Lid']);
		const lid = blockInst.instances[1];
		expectClose(lid.placement.translation_m, [0.05, 0, 0]);
		expectClose(lid.transform.rotation_quat, [0, 0, Math.SQRT1_2, Math.SQRT1_2]);
		// The Assembly panel shows what the agent placed.
		await expect(page.getByTestId('asm-instance-1')).toBeVisible();
		expect(await page.evaluate(() => window.__waffle.getAssemblyStatus().placements)).toHaveProperty(lid.id);

		// Connectors from the parts' faces: the plate's top and the lid's bottom.
		const top = ok(await relay.callTool('connector_add', { instance_path: [baseInst.instance_id], geom_ref: base.top, name: 'plate top' }));
		const topC = top.connectors.find((c) => c.id === top.connector_id);
		expect(topC.world_frame.kind).toBe('planar face');
		expectClose(topC.world_frame.origin, base.topCentre);
		expectClose(topC.world_frame.z_axis, [0, 0, 1]);
		const bottom = ok(await relay.callTool('connector_add', { instance_path: [blockInst.instance_id], geom_ref: blk.bottom }));
		const bottomC = bottom.connectors.find((c) => c.id === bottom.connector_id);
		expect(bottomC.name).toBe('Lid connector 2');
		// The lid's own bottom centre, placed: turned 90° about z and moved +50 mm in x.
		expectClose(bottomC.world_frame.origin, apply(lid.placement, blk.bottomCentre));
		expectClose(bottomC.world_frame.z_axis, [0, 0, -1]);

		// Refusals name what is missing.
		refused(await relay.callTool('connector_add', { instance_path: ['00000000-0000-4000-8000-000000000000'] }), 'InstanceNotFound');
		refused(await relay.callTool('connector_add', { instance_path: [baseInst.instance_id], part_connector: '00000000-0000-4000-8000-000000000000' }), 'ConnectorNotFound');
		refused(await relay.callTool('mate_add', { a: top.connector_id, b: top.connector_id }), 'InvalidArguments');
		refused(await relay.callTool('mate_edit', { mate_id: '00000000-0000-4000-8000-000000000000', name: 'x' }), 'MateNotFound');

		// Fastened, flipped: the lid's bottom lands on the plate's top.
		const mate = ok(await relay.callTool('mate_add', { a: top.connector_id, b: bottom.connector_id }));
		expect(mate.mates.map((m) => [m.name, m.kind, m.connectors])).toEqual([
			['Fastened 1', { type: 'Fastened', flip: true }, [top.connector_id, bottom.connector_id]]
		]);
		expect(mate.errors).toEqual([]);
		const placed = mate.instances.find((i) => i.id === blockInst.instance_id).placement;
		const onTop = base.topCentre;
		expectClose(apply(placed, blk.bottomCentre), onTop);
		expectClose(apply(placed, blk.topCentre), [onTop[0], onTop[1], onTop[2] + 0.005]);
		const placedBottom = mate.connectors.find((c) => c.id === bottom.connector_id).world_frame;
		expectClose(placedBottom.origin, onTop);
		expectClose(placedBottom.z_axis, [0, 0, -1]);
		// The grounded plate did not move.
		expectClose(mate.instances.find((i) => i.id === baseInst.instance_id).placement.translation_m, [0, 0, 0]);

		// A 0-degree Fastened turn re-solves to the same place; a 45 mm offset along the
		// plate connector's z lifts the lid by that much.
		const lifted = ok(await relay.callTool('connector_edit', { connector_id: top.connector_id, offset_m: [0, 0, 0.045] }));
		expectClose(apply(lifted.instances.find((i) => i.id === blockInst.instance_id).placement, blk.bottomCentre), [onTop[0], onTop[1], onTop[2] + 0.045]);
		expect(lifted.connectors.find((c) => c.id === top.connector_id).offset_m).toEqual([0, 0, 0.045]);
		ok(await relay.callTool('connector_edit', { connector_id: top.connector_id, offset_m: [0, 0, 0] }));

		// Suppressing the mate frees the lid back to its own transform.
		let s = ok(await relay.callTool('mate_edit', { mate_id: mate.mate_id, suppressed: true }));
		expectClose(s.instances.find((i) => i.id === blockInst.instance_id).placement.translation_m, [0.05, 0, 0]);
		s = ok(await relay.callTool('mate_edit', { mate_id: mate.mate_id, suppressed: false, name: 'Lid on plate' }));
		expect(s.mates[0]).toMatchObject({ name: 'Lid on plate', suppressed: false });
		expectClose(apply(s.instances.find((i) => i.id === blockInst.instance_id).placement, blk.bottomCentre), onTop);

		// Saved with the document and read back, ids and all.
		ok(await relay.callTool('document_save'));
		const other = ok(await relay.callTool('document_new', { name: 'Elsewhere' }));
		expect(other.storage_id).not.toBe(doc.storage_id);
		const reopened = ok(await relay.callTool('document_open', { id: doc.storage_id }));
		expect(reopened.tabs.map((t) => t.kind)).toEqual(['Part', 'Part', 'Assembly']);
		ok(await relay.callTool('tab_switch', { tab_id: asm.tab_id }));
		const back = ok(await relay.callTool('assembly_get'));
		expect(back.instances.map((i) => i.id)).toEqual([baseInst.instance_id, blockInst.instance_id]);
		expect(back.connectors.map((c) => c.id)).toEqual([top.connector_id, bottom.connector_id]);
		expect(back.mates.map((m) => [m.id, m.name])).toEqual([[mate.mate_id, 'Lid on plate']]);
		expectClose(apply(back.instances[1].placement, blk.bottomCentre), onTop);

		// Deletes cascade the way the panel's do.
		const noMate = ok(await relay.callTool('mate_delete', { mate_id: mate.mate_id }));
		expect(noMate.mates).toEqual([]);
		expect(noMate.connectors).toHaveLength(2);
		const noLid = ok(await relay.callTool('instance_delete', { instance_id: blockInst.instance_id }));
		expect(noLid.instances.map((i) => i.id)).toEqual([baseInst.instance_id]);
		expect(noLid.connectors.map((c) => c.id)).toEqual([top.connector_id]);
		refused(await relay.callTool('instance_edit', { instance_id: blockInst.instance_id, name: 'gone' }), 'InstanceNotFound');
		const none = ok(await relay.callTool('connector_delete', { connector_id: top.connector_id }));
		expect(none.connectors).toEqual([]);

		// Back on a Part tab the feature tools work again and the assembly tools refuse.
		ok(await relay.callTool('tab_switch', { tab_id: blockTab }));
		expect(ok(await relay.callTool('model_summary')).features.map((f) => f.kind)).toEqual(['Sketch', 'Extrude']);
		refused(await relay.callTool('assembly_get'), 'TabKindNotSupported');
		expectNoAnyCrash(crashes);
	});

	test('a connector from a part\'s named MateConnector feature, and an explicit frame', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const doc = ok(await relay.callTool('document_new', { name: 'Named' }));
		const partTab = doc.active_tab;
		const faces = await block(0.01, 0.01, 0.005);
		// A named connector on the part's top face (specs/part_mate_connectors.md).
		const named = ok(
			await relay.callTool('feature_add', {
				operation: { type: 'MateConnector', params: { name: 'lid top', geom_ref: faces.top } }
			})
		);
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.connectors.map((c) => c.name)).toEqual(['lid top']);

		ok(await relay.callTool('tab_add', { kind: 'Assembly' }));
		const a = ok(await relay.callTool('instance_add', { tab_id: partTab }));
		const b = ok(await relay.callTool('instance_add', { tab_id: partTab, transform: { translation_m: [0.1, 0.1, 0.1] } }));
		// The placed parts' named connectors, in world coordinates.
		expect(b.part_connectors.map((p) => [p.feature_id, p.instance_path])).toEqual([
			[named.feature_id, [a.instance_id]],
			[named.feature_id, [b.instance_id]]
		]);
		expectClose(b.part_connectors[1].origin, faces.topCentre.map((v) => v + 0.1));

		const fromNamed = ok(
			await relay.callTool('connector_add', { instance_path: [a.instance_id], part_connector: named.feature_id })
		);
		const c1 = fromNamed.connectors[0];
		expect(c1.part_connector).toBe(named.feature_id);
		expect(c1.name).toBe('Part 1 1 › lid top');
		expectClose(c1.world_frame.origin, faces.topCentre);

		// An explicit frame on the other instance: its bottom-face centre, z down.
		const explicit = ok(
			await relay.callTool('connector_add', {
				instance_path: [b.instance_id],
				frame: { origin: faces.bottomCentre, z_axis: [0, 0, -1], x_axis: [1, 0, 0] },
				name: 'underside'
			})
		);
		const c2 = explicit.connectors[1];
		expect(c2.world_frame.kind).toBeNull();
		expectClose(c2.world_frame.origin, faces.bottomCentre.map((v) => v + 0.1));

		// Stack b on a, then turn it 90° with the mate's rotation: the x axes tell.
		const mate = ok(await relay.callTool('mate_add', { a: c1.id, b: c2.id, kind: 'Fastened', rotation_deg: 90 }));
		expect(mate.errors).toEqual([]);
		const placed = mate.instances[1].placement;
		expectClose(apply(placed, faces.bottomCentre), faces.topCentre);
		const under = mate.connectors[1].world_frame;
		expectClose(under.z_axis, [0, 0, -1]);
		const topX = mate.connectors[0].world_frame.x_axis;
		const dot = under.x_axis[0] * topX[0] + under.x_axis[1] * topX[1] + under.x_axis[2] * topX[2];
		expect(Math.abs(dot)).toBeLessThanOrEqual(1e-9);

		// The Revolute kind frees the turn: re-kind, and the state says so.
		const rev = ok(await relay.callTool('mate_edit', { mate_id: mate.mate_id, kind: 'Revolute' }));
		expect(rev.mates[0].kind).toEqual({ type: 'Revolute', flip: true });
		expect(rev.errors).toEqual([]);
		expectClose(apply(rev.instances[1].placement, faces.bottomCentre), faces.topCentre, 1e-6);
		expectNoAnyCrash(crashes);
	});

	test('assembly edits sent concurrently all land: the page runs them one at a time', async ({ page }) => {
		// Measured 2026-09-18 building a bicycle over the link: connectors
		// added in one parallel burst vanished, each in-flight edit sending
		// the tab copy without the others' additions and the slower answer
		// overwriting the faster one's. The document commands now queue.
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const doc = ok(await relay.callTool('document_new', { name: 'Burst' }));
		const partTab = doc.active_tab;
		await block(0.01, 0.01, 0.005);
		ok(await relay.callTool('tab_add', { kind: 'Assembly' }));
		const inst = ok(await relay.callTool('instance_add', { tab_id: partTab, fixed: true }));

		const names = ['c1', 'c2', 'c3', 'c4', 'c5', 'c6'];
		const results = await Promise.all(
			names.map((name, i) =>
				relay.callTool('connector_add', {
					instance_path: [inst.instance_id],
					frame: { origin: [0.001 * i, 0, 0], z_axis: [0, 0, 1], x_axis: [1, 0, 0] },
					name
				})
			)
		);
		for (const r of results) ok(r);
		const state = ok(await relay.callTool('assembly_get'));
		expect(state.connectors.map((c) => c.name).sort()).toEqual(names);
		// Every answer described a state that contained its own connector.
		for (const r of results) {
			const s = r.structuredContent;
			expect(s.connectors.map((c) => c.id)).toContain(s.connector_id);
		}
		expectNoAnyCrash(crashes);
	});
});
