/**
 * Agent link O3 parity (specs/waffle_mcp_server.md I1, §5 O3): for each scripted
 * sequence, the document an agent builds through the REAL relay equals the
 * document a fresh page builds from the same bridge messages sent through the
 * store's agent entry point (`__waffle.replayEngineMessages`). Canonical form:
 * UUIDs renamed in structural order, timestamps and the derived thumbnail
 * dropped, keys sorted.
 */
import { test, expect } from '@playwright/test';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const AGENT = 'agent-parity-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];
const SQUARE_AT = (x) => [P(1, x, 0), P(2, x + 0.01, 0), P(3, x + 0.01, 0.01), P(4, x, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];
const CIRCLE = [P(1, 0, 0), { type: 'Circle', id: 2, center_id: 1, radius: 0.005 }];
const D_SHAPE = [P(1, 0, 0), P(2, 0.01, 0), P(3, -0.01, 0), { type: 'Arc', id: 4, center_id: 1, start_id: 2, end_id: 3 }, L(5, 3, 2)];
const CONFLICT = [
	{ type: 'HDistance', point_a: 1, point_b: 2, value: 0.02 },
	{ type: 'HDistance', point_a: 1, point_b: 2, value: 0.03 }
];

const extrude = (sketchId, depth, ids = [5, 6, 7, 8], extra = {}) => ({
	type: 'Extrude',
	params: { sketch_id: sketchId, profile_index: 0, profile_entity_ids: ids, depth, symmetric: false, cut: false, ...extra }
});

/**
 * A sequence is a list of steps; each step is `(call, results) => Promise`,
 * where `call(name, args)` runs one tool and `results` holds earlier answers.
 * @type {Array<[string, Array<(call: (name: string, args?: object) => Promise<any>, r: any[]) => Promise<any>>]>}
 */
const SEQUENCES = [
	['sketch', [(call) => call('sketch_create', { plane: XY, entities: RECT })]],
	['sketch + extrude', [(call) => call('sketch_create', { plane: XY, entities: RECT }), (call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005) })]],
	['extrude + rename', [
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005) }),
		(call, r) => call('feature_rename', { feature_id: r[1].feature_id, new_name: 'Base plate' })
	]],
	['extrude + suppress', [
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005) }),
		(call, r) => call('feature_suppress', { feature_id: r[1].feature_id, suppressed: true })
	]],
	['delete a depended-on sketch', [
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005) }),
		(call, r) => call('feature_delete', { feature_id: r[0].feature_id })
	]],
	['rolled-back step, then a good one', [
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005, [99]) }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005) })
	]],
	['kept failing step', [
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005, [99]), on_error: 'keep' })
	]],
	['cylinder + undo', [
		(call) => call('sketch_create', { plane: XY, entities: CIRCLE }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.01, [2]) }),
		(call) => call('undo')
	]],
	['cylinder + undo + redo', [
		(call) => call('sketch_create', { plane: XY, entities: CIRCLE }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.01, [2]) }),
		(call) => call('undo'),
		(call) => call('redo')
	]],
	['parameter-driven depth', [
		(call) => call('parameters_set', { parameters: [{ name: 'w', expression: '20' }] }),
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[1].feature_id, 0.005, [5, 6, 7, 8], { depth_expr: 'w / 4' }) })
	]],
	['two bodies + reorder', [
		(call) => call('sketch_create', { plane: XY, entities: SQUARE_AT(0) }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005, [5, 6, 7, 8], { combine: { type: 'NewBody' } }) }),
		(call) => call('sketch_create', { plane: XY, entities: SQUARE_AT(0.05) }),
		(call, r) => call('feature_add', { operation: extrude(r[2].feature_id, 0.005, [5, 6, 7, 8], { combine: { type: 'NewBody' } }) }),
		(call, r) => call('feature_reorder', { feature_id: r[2].feature_id, new_position: 0 })
	]],
	['rollback bar', [
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005) }),
		(call) => call('rollback_set', { index: 0 })
	]],
	['edit depth', [
		(call) => call('sketch_create', { plane: XY, entities: RECT }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.005) }),
		async (call, r) => {
			const op = (await call('feature_get', { feature_id: r[1].feature_id })).operation;
			op.params.depth = 0.01;
			return call('feature_edit', { feature_id: r[1].feature_id, operation: op });
		}
	]],
	['failed solves: kept, then rolled back', [
		(call) => call('sketch_create', { plane: XY, entities: RECT, constraints: CONFLICT, on_error: 'keep' }),
		(call) => call('sketch_create', { plane: XY, entities: RECT, constraints: CONFLICT })
	]],
	['arc profile + body rename', [
		(call) => call('sketch_create', { plane: XY, entities: D_SHAPE }),
		(call, r) => call('feature_add', { operation: extrude(r[0].feature_id, 0.004, [4, 5]) }),
		(call, r) => call('body_rename', { body_id: r[1].bodies_added[0], new_name: 'D' })
	]]
];

const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g;

/**
 * The saved document with UUIDs renamed in structural order (document, tabs,
 * then each feature and what it contains, in tree order), volatile fields
 * dropped, keys sorted.
 * @param {import('@playwright/test').Page} page
 */
async function canonicalDocument(page) {
	const doc = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
	const names = new Map();
	const see = (text) => {
		for (const m of String(text).matchAll(UUID)) if (!names.has(m[0])) names.set(m[0], `uuid-${names.size + 1}`);
	};
	see(doc.document?.id);
	for (const tab of doc.tabs ?? []) {
		see(tab.id);
		for (const feature of tab.kind?.features?.features ?? []) see(JSON.stringify(feature));
		for (const parameter of tab.kind?.features?.parameters ?? []) see(JSON.stringify(parameter));
	}
	const DROP = new Set(['modified', 'created', 'at', 'preview_mesh']);
	const canon = (v) =>
		Array.isArray(v)
			? v.map(canon)
			: v && typeof v === 'object'
				? Object.fromEntries(Object.keys(v).filter((k) => !DROP.has(k)).map((k) => [k, canon(v[k])]))
				: v;
	let text = JSON.stringify(canon(doc));
	see(text); // anything not reached structurally, in document order
	text = text.replace(UUID, (u) => names.get(u));
	const sortKeys = (v) =>
		Array.isArray(v) ? v.map(sortKeys) : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sortKeys(v[k])])) : v;
	return JSON.stringify(sortKeys(JSON.parse(text)));
}

test.describe('Agent link O3 parity', () => {
	for (const [name, steps] of SEQUENCES) {
		test(`O3: ${name}`, async ({ page, context, baseURL }) => {
			const origin = new URL(/** @type {string} */ (baseURL)).origin;
			const relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
			try {
				await relay.waitListening();
				await relay.initialize(AGENT);
				const crashes = collectCrashErrors(page);
				await pairAgent(page, relay, AGENT);

				await page.evaluate(() => window.__waffle.recordEngineSends(true, { payloads: true }));
				const results = [];
				const call = async (tool, args = {}) => (await relay.callTool(tool, args)).structuredContent;
				for (const step of steps) results.push(await step(call, results));
				const log = await page.evaluate(() => window.__waffle.getEngineSendLog());
				const agentMessages = log.filter((e) => e.origin === 'agent');
				expect(agentMessages.length).toBeGreaterThan(0);
				expect(log.filter((e) => e.origin === 'user')).toEqual([]);
				const viaAgent = await canonicalDocument(page);

				const fresh = await context.newPage();
				const freshCrashes = collectCrashErrors(fresh);
				await fresh.goto('/');
				await fresh.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 30000 });
				await fresh.evaluate((entries) => window.__waffle.replayEngineMessages(entries), agentMessages);
				const viaStore = await canonicalDocument(fresh);

				expect(viaStore).toBe(viaAgent);
				expectNoAnyCrash(crashes);
				expectNoAnyCrash(freshCrashes);
			} finally {
				expect(await relay.close()).toBe(0);
			}
		});
	}
});
