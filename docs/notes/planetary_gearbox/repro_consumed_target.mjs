/**
 * Repro: an explicit Strict combine target that names an output ALREADY
 * CONSUMED by an earlier combine. Expected (rebuild.rs resolve_combine_targets
 * doc): a loud ResolutionFailed. Observed in the gearbox run: a separate body.
 * This measures what that body is.
 */
import { createRequire } from 'node:module';
import { McpRelay, pairAgent, relayTestPort } from '/home/claude/workspace/app/tests/gui/helpers/mcp-relay.js';
const require = createRequire('/home/claude/workspace/app/package.json');
const { chromium } = require('playwright-core');

const P = (id, x, y) => ({ type: 'Point', id, x, y });
const C = (id, center_id, radius) => ({ type: 'Circle', id, center_id, radius });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
const solidRef = (feature_id) => ({
	kind: { type: 'Solid' },
	anchor: { type: 'FeatureOutput', feature_id, output_key: { type: 'Main' } },
	selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
	policy: { type: 'Strict' }
});
const ext = (sketch_id, depth, combine, targets) => ({
	type: 'Extrude',
	params: { sketch_id, profile_index: 0, depth, direction: null, symmetric: false, cut: combine === 'Cut', merge: false, target_body: null, depth_mode: { type: 'Blind' }, combine: { type: combine }, targets }
});

const origin = 'http://localhost:5173';
const relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
await relay.waitListening();
await relay.initialize('repro');
const browser = await chromium.launch({ args: ['--use-gl=swiftshader'] });
const page = await browser.newPage();
const call = async (t, a) => {
	const r = await relay.callTool(t, a, 120000);
	if (r.isError) console.log(`  ${t} -> ERROR ${JSON.stringify(r.structuredContent.error)}`);
	return r.structuredContent;
};
try {
	await pairAgent(page, relay, 'repro');
	await call('document_new', { name: 'consumed-target repro' });
	const plateS = await call('sketch_create', { plane: XY, entities: [P(1, 0, 0), C(2, 1, 0.02)] });
	const plate = await call('feature_add', { operation: ext(plateS.feature_id, 0.004, 'NewBody', []) });
	const plateV = (await call('body_measure', { body_id: plate.bodies_added[0] })).volume_m3;
	console.log(`plate volume ${(plateV * 1e9).toFixed(1)} mm³`);
	const pin = async (x) => {
		const s = await call('sketch_create', { plane: { origin: [0, 0, 0.002], normal: [0, 0, 1] }, entities: [P(1, x, 0), C(2, 1, 0.002)] });
		return call('feature_add', { operation: ext(s.feature_id, 0.010, 'Add', [solidRef(plate.feature_id)]) });
	};
	const a = await pin(0.01);
	console.log('pin 1 (targets plate):', JSON.stringify({ added: a.bodies_added, removed: a.bodies_removed, warnings: a.warnings, error: a.error }));
	const b = await pin(-0.01);
	console.log('pin 2 (targets the now-CONSUMED plate):', JSON.stringify({ added: b.bodies_added, removed: b.bodies_removed, warnings: b.warnings, error: b.error }));
	const summary = await call('model_summary');
	for (const body of summary.bodies) {
		const m = await call('body_measure', { body_id: body.body_id });
		console.log(`  body ${body.name}: V=${(m.volume_m3 * 1e9).toFixed(1)} mm³ faces=${m.face_count} (plate alone = ${(plateV * 1e9).toFixed(1)}, one pin ≈ ${(Math.PI * 4 * 8).toFixed(1)} above the plate)`);
	}
	console.log('errors:', JSON.stringify(summary.errors), 'warnings:', JSON.stringify(summary.warnings));
} finally {
	await browser.close();
	await relay.close();
}
