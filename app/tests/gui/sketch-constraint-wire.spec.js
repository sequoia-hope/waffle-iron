/**
 * Every constraint the sketch UI's constraint builders emit, after the bridge
 * mapping, must be a `SketchConstraint` the engine can parse (REVIEW.md
 * §3.2-1). Pure: no browser. The oracle is the generated engine schema
 * (`engineSchemas.generated.js`, from waffle-types' JsonSchema derive) — a
 * variant whose `type` the engine knows, every required field present with
 * the right JSON type, and no field the engine does not define.
 *
 * A builder that misses here fails EVERY later solve in the session, because
 * SolveSketch resends the whole constraint list.
 */
import { test, expect } from '@playwright/test';
import { getApplicableConstraints } from '../../src/lib/sketch/constraintLogic.js';
import { mapConstraintForBridge } from '../../src/lib/sketch/bridgeConstraint.js';
import { ENGINE_DEFS } from '../../src/lib/agent/tools/engineSchemas.generated.js';

const VARIANTS = new Map(ENGINE_DEFS.SketchConstraint.oneOf.map((v) => [v.properties.type.const, v]));

/** Problems with `c` against the engine schema; [] when it parses. */
function wireProblems(c) {
	const v = VARIANTS.get(c.type);
	if (!v) return [`unknown variant ${c.type}`];
	const out = [];
	for (const k of v.required) if (c[k] == null) out.push(`${c.type}: missing ${k}`);
	for (const [k, val] of Object.entries(c)) {
		const prop = v.properties[k];
		if (!prop) { out.push(`${c.type}: unknown field ${k}`); continue; }
		const types = [prop.type].flat();
		const ok = types.some((t) =>
			(t === 'integer' && Number.isInteger(val)) ||
			(t === 'number' && typeof val === 'number' && Number.isFinite(val)) ||
			(t === 'string' && typeof val === 'string') ||
			(t === 'boolean' && typeof val === 'boolean') ||
			(t === 'null' && val === null) ||
			(t === 'array' && Array.isArray(val)));
		if (prop.type && !ok) out.push(`${c.type}: ${k}=${JSON.stringify(val)} is not ${types.join('|')}`);
	}
	return out;
}

// A small sketch with every entity kind the builders dispatch on.
const entities = [
	{ type: 'Point', id: 1 }, { type: 'Point', id: 2 }, { type: 'Point', id: 3 }, { type: 'Point', id: 4 },
	{ type: 'Point', id: 5 }, { type: 'Point', id: 6 }, { type: 'Point', id: 7 }, { type: 'Point', id: 8 },
	{ type: 'Line', id: 10, start_id: 1, end_id: 2 },
	{ type: 'Line', id: 11, start_id: 3, end_id: 4 },
	{ type: 'Circle', id: 20, center_id: 5, radius: 0.01 },
	{ type: 'Circle', id: 21, center_id: 6, radius: 0.02 },
	{ type: 'Arc', id: 30, center_id: 7, start_id: 8, end_id: 1 },
];
const positions = new Map([
	[1, { x: 0, y: 0 }], [2, { x: 0.03, y: 0.01 }], [3, { x: -0.01, y: 0.02 }], [4, { x: 0.02, y: 0.05 }],
	[5, { x: 0.05, y: 0 }], [6, { x: 0.08, y: 0.01 }], [7, { x: 0.01, y: -0.03 }], [8, { x: 0.02, y: -0.03 }],
]);

// Every selection shape a builder answers to.
const SELECTIONS = {
	'1 line': [10],
	'2 points': [3, 4],
	'2 points (coincident x)': [1, 1],
	'2 lines': [10, 11],
	'point + line': [3, 10],
	'point + circle': [3, 20],
	'point + arc': [3, 30],
	'1 circle': [20],
	'1 arc': [30],
	'2 circles': [20, 21],
	'circle + arc': [20, 30],
	'line + arc': [10, 30],
	'line + circle': [10, 20],
	'2 points + line': [3, 4, 10],
	'1 point': [3],
};

test('every constraint builder emits a SketchConstraint the engine parses', () => {
	const problems = [];
	let checked = 0;
	for (const [shape, ids] of Object.entries(SELECTIONS)) {
		const builders = getApplicableConstraints(new Set(ids), entities, positions);
		for (const [name, build] of Object.entries(builders)) {
			if (!build) continue;
			const wire = mapConstraintForBridge(build());
			checked++;
			for (const p of wireProblems(wire)) problems.push(`${shape} → ${name}: ${p}`);
		}
	}
	expect(checked).toBeGreaterThan(25);
	expect(problems).toEqual([]);
});

test('Symmetric stores its line as symmetry_line; a legacy entity_c is translated', () => {
	const b = getApplicableConstraints(new Set([3, 4, 10]), entities, positions);
	expect(b.symmetric()).toEqual({ type: 'Symmetric', entity_a: 3, entity_b: 4, symmetry_line: 10 });
	expect(mapConstraintForBridge({ type: 'Symmetric', entity_a: 3, entity_b: 4, entity_c: 10 }))
		.toEqual({ type: 'Symmetric', entity_a: 3, entity_b: 4, symmetry_line: 10 });
});

test('the bridge lowers UI-only spellings', () => {
	for (const c of [
		{ type: 'WhereDragged', point: 1, x: 0.1, y: 0.2 },
		{ type: 'WhereDragged', point: 1, x: 0.1, y: 0.2, _isDrag: true },
		{ type: 'EqualRadius', entity_a: 20, entity_b: 21 },
		{ type: 'LengthRatio', entity_a: 10, entity_b: 11, value: 2 },
	]) {
		expect(wireProblems(mapConstraintForBridge(c)), JSON.stringify(c)).toEqual([]);
	}
});
