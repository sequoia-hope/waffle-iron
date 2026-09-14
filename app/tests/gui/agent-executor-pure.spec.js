/**
 * Agent link (specs/waffle_mcp_server.md): the executor's pure helpers, tested
 * without a browser — the ModelDelta and rollback check (§2.5, I3), the A13
 * sketch shape checks, and the engine-schema closure the tool manifest embeds
 * (§2.4, O19).
 */
import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { modelDelta, newlyErroring, sameModel, takeSnapshot } from '../../src/lib/agent/delta.js';
import { sketchInputProblem } from '../../src/lib/agent/sketchInput.js';
import { defsFor } from '../../src/lib/agent/tools/engineSchemas.js';
import { TOOLS } from '../../src/lib/agent/tools/index.js';
import { canonicalJson } from '../../src/lib/agent/tools/manifest.js';

const golden = JSON.parse(readFileSync(new URL('../../../docs/schema/waffle-v5.schema.json', import.meta.url), 'utf8'));

const feature = (id, extra = {}) => ({ id, name: id, suppressed: false, operation: { type: 'Extrude', params: { depth: 1 } }, ...extra });

function snap({ features = [], errors = [], bodies = [], provenance = {} } = {}) {
	return takeSnapshot({
		featureTree: { features, active_index: null, provenance },
		featureErrors: new Map(errors),
		bodies: bodies.map((bodyId) => ({ bodyId }))
	});
}

test.describe('agent delta', () => {
	test('added, removed, changed and body lists', () => {
		const before = snap({ features: [feature('a'), feature('b')], bodies: ['a/main'] });
		const after = snap({
			features: [feature('a', { name: 'renamed' }), feature('c')],
			bodies: ['c/main']
		});
		const delta = modelDelta(before, after, { warnings: ['w'] });
		expect(delta.features_added).toEqual(['c']);
		expect(delta.features_removed).toEqual(['b']);
		expect(delta.features_changed).toEqual(['a']);
		expect(delta.order_changed).toBe(false);
		expect(delta.bodies_added).toEqual(['c/main']);
		expect(delta.bodies_removed).toEqual(['a/main']);
		expect(delta.warnings).toEqual(['w']);
	});

	test('a reorder is order_changed, not a content change', () => {
		const before = snap({ features: [feature('a'), feature('b'), feature('c')] });
		const after = snap({ features: [feature('b'), feature('c'), feature('a')] });
		const delta = modelDelta(before, after);
		expect(delta.order_changed).toBe(true);
		expect(delta.features_changed).toEqual([]);
	});

	test('a provenance change is a change', () => {
		const before = snap({ features: [feature('a')] });
		const after = snap({ features: [feature('a')], provenance: { a: { origin: { type: 'Agent', name: 'x' } } } });
		expect(modelDelta(before, after).features_changed).toEqual(['a']);
	});

	test('errors are current, in tree order, typed when known', () => {
		const after = snap({ features: [feature('a'), feature('b')], errors: [['b', 'boom'], ['a', 'bad']] });
		const delta = modelDelta(snap(), after, { typedErrors: [{ feature_id: 'b', kind: { type: 'NotSupported', operation: 'x' }, message: 'boom' }] });
		expect(delta.errors).toEqual([
			{ feature_id: 'a', message: 'bad' },
			{ feature_id: 'b', message: 'boom', kind: { type: 'NotSupported', operation: 'x' } }
		]);
	});

	test('newlyErroring ignores a persisting error and reports a changed one', () => {
		const before = snap({ features: [feature('a'), feature('b')], errors: [['a', 'old'], ['b', 'same']] });
		const after = snap({ features: [feature('a'), feature('b')], errors: [['a', 'new'], ['b', 'same']] });
		expect(newlyErroring(before, after)).toEqual([{ feature_id: 'a', message: 'new' }]);
	});

	test('sameModel compares canonical trees (key order does not matter)', () => {
		const a = snap({ features: [feature('a')], provenance: { a: { origin: { type: 'Agent', name: 'x' } } } });
		const b = takeSnapshot({
			featureTree: { provenance: { a: { origin: { name: 'x', type: 'Agent' } } }, active_index: null, features: [feature('a')] },
			featureErrors: new Map(),
			bodies: []
		});
		expect(sameModel(a, b)).toBe(true);
		expect(sameModel(a, snap({ features: [feature('a', { suppressed: true })] }))).toBe(false);
	});

	test('a snapshot is detached from later mutation', () => {
		const tree = { features: [feature('a')], active_index: null };
		const s = takeSnapshot({ featureTree: tree, featureErrors: new Map(), bodies: [] });
		tree.features[0].name = 'mutated';
		expect(s.tree.features[0].name).toBe('a');
	});
});

test.describe('agent sketch input (A13)', () => {
	const rect = [
		{ type: 'Point', id: 1, x: 0, y: 0 },
		{ type: 'Point', id: 2, x: 1, y: 0 },
		{ type: 'Line', id: 3, start_id: 1, end_id: 2 }
	];

	test('well formed input passes', () => {
		expect(sketchInputProblem(rect, [{ type: 'Horizontal', entity: 3 }])).toBeNull();
	});

	test('duplicate id', () => {
		expect(sketchInputProblem([...rect, { type: 'Point', id: 2, x: 5, y: 5 }], [])).toBe('/entities/3/id: duplicate entity id 2');
	});

	test('a line endpoint that is not a point', () => {
		expect(sketchInputProblem([...rect, { type: 'Line', id: 4, start_id: 3, end_id: 1 }], [])).toBe(
			'/entities/3/start_id: no Point entity with id 3'
		);
	});

	test('a spline control point that does not exist', () => {
		expect(sketchInputProblem([...rect, { type: 'Spline', id: 4, point_ids: [1, 9] }], [])).toBe(
			'/entities/3/point_ids/1: no Point entity with id 9'
		);
	});

	test('a constraint naming a missing entity', () => {
		expect(sketchInputProblem(rect, [{ type: 'Coincident', point_a: 1, point_b: 42 }])).toBe(
			'/constraints/0/point_b: no entity with id 42'
		);
	});
});

test.describe('agent tool schemas (§2.4, O19)', () => {
	test('defsFor returns the transitive closure, byte-equal to the golden', () => {
		const defs = defsFor('GeomRef');
		expect(Object.keys(defs)).toContain('Anchor');
		expect(Object.keys(defs)).toContain('TopoSignature');
		for (const [name, def] of Object.entries(defs)) {
			expect(canonicalJson(def)).toBe(canonicalJson(golden.$defs[name]));
		}
	});

	test('every $ref in every tool input schema resolves in its own $defs', () => {
		const refs = (node) =>
			node && typeof node === 'object'
				? [...(typeof node.$ref === 'string' ? [node.$ref] : []), ...Object.values(node).flatMap(refs)]
				: [];
		for (const tool of TOOLS) {
			const defs = tool.inputSchema.$defs ?? {};
			for (const ref of refs(tool.inputSchema)) {
				expect(ref.startsWith('#/$defs/'), `${tool.name}: ${ref}`).toBe(true);
				expect(ref.slice('#/$defs/'.length) in defs, `${tool.name}: ${ref}`).toBe(true);
			}
		}
	});

	test('an unknown definition is a loud error', () => {
		expect(() => defsFor('NoSuchType')).toThrow('engine schema has no definition NoSuchType');
	});
});
