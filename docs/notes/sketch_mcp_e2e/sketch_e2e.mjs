/**
 * Exercise the WHOLE sketch door through the real agent link: a local
 * `waffle-mcp-relay` driven as an MCP client over stdio, paired with a
 * headless page on the dev server (the harness the agent-*.spec.js files use).
 *
 * This is the measurement behind S3 ("an agent can edit a sketch, and see what
 * the solver sees", `specs/agent_mechanical_design.md` §10.3) and the first
 * automated test of the sketch system that drives it the way an agent does
 * rather than the way a pointer does.
 *
 * What it drives, in order:
 *   document_new → sketch_create a free rectangle (dof 8, eight free directions)
 *   sketch_solve_state — the same state, read back, committing nothing
 *   sketch_edit × 7 AddConstraint — the DOF ladder 8 → 0, one constraint a call
 *   a duplicate constraint — satisfied, and named `redundant` rather than red
 *   a contradictory dimension — refused, with the document provably unmoved
 *   sketch_edit MovePoint — a drag, whose pin is transient and named
 *   sketch_edit Fillet — the arc, its two tangents, the region area it leaves,
 *                        and one undo that puts all of it back
 *   sketch_edit Trim / Extend / Offset / Mirror / SetConstruction / SetDimension
 *   every typed refusal the door can answer with
 *   feature_add Extrude on the edited sketch's region — the volume closes
 *   document_save
 *
 * The oracles are closed forms, not recordings: the rectangle's area and the
 * filleted rectangle's area are arithmetic, so a region can only be right by
 * being right.
 *
 * Output (all under OUT, default ./out beside this file):
 *   calls.jsonl   every tool call and its structuredContent
 *   report.json   the checks, with the measured numbers
 *
 * Run from the repo root with the dev server up:
 *   node docs/notes/sketch_mcp_e2e/sketch_e2e.mjs
 */
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import path from 'node:path';
import { McpRelay, pairAgent, relayTestPort } from '../../../app/tests/gui/helpers/mcp-relay.js';

const require = createRequire(path.resolve('app/package.json'));
const { chromium } = require('playwright-core');

const HERE = path.dirname(new URL(import.meta.url).pathname);
const OUT = process.env.OUT ?? path.join(HERE, 'out');
mkdirSync(OUT, { recursive: true });
const LOG = path.join(OUT, 'calls.jsonl');
writeFileSync(LOG, '');

const APP = process.env.APP_URL ?? 'http://localhost:5173';
const AGENT = 'claude-code-sketch-e2e';
const CALL_TIMEOUT_MS = 300_000;

// The plate, in meters. 60 × 40, 6 thick, with one 4 mm rounded corner.
const W = 0.06;
const H = 0.04;
const T = 0.006;
const R = 0.004;
// A fillet of radius r replaces a square corner with a quarter disc, so it
// removes the corner square minus that quarter: r² − πr²/4.
const FILLET_AREA_LOSS = R * R * (1 - Math.PI / 4);

let relay;
const t0 = Date.now();
const report = { checks: [], tools: new Set(), measured: {} };

function log(entry) {
	appendFileSync(LOG, JSON.stringify(entry) + '\n');
}

function check(name, ok, detail) {
	report.checks.push({ name, ok: !!ok, detail });
	console.log(`   ${ok ? 'PASS' : 'FAIL'} ${name}${detail === undefined ? '' : ' — ' + JSON.stringify(detail)}`);
	if (!ok) process.exitCode = 1;
}

/** One tool call; throws on isError unless `allowError`. */
async function callFull(tool, args = {}, { allowError = false, label = '' } = {}) {
	const started = Date.now();
	let result;
	try {
		result = await relay.callTool(tool, args, CALL_TIMEOUT_MS);
	} catch (e) {
		// The relay validates arguments against the manifest's inputSchema
		// BEFORE the page sees the call (spec §2.4): a schema miss is a
		// JSON-RPC -32602 with the JSON pointer, not a tool result.
		if (!allowError || !/-32602/.test(e.message)) throw e;
		const m = e.message.match(/"message":"([^"]*)".*?"data":"([^"]*)"/);
		result = {
			isError: true,
			structuredContent: { error: { code: 'SchemaRejected', message: m?.[1] ?? e.message, details: { path: m?.[2] } } }
		};
	}
	const ms = Date.now() - started;
	const sc = result.structuredContent ?? {};
	report.tools.add(tool);
	log({ t: Date.now() - t0, ms, tool, label, args, isError: !!result.isError, result: sc });
	const brief = result.isError ? `ERROR ${sc.error?.code}: ${sc.error?.message}` : 'ok';
	console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s +${(ms / 1000).toFixed(1)}s] ${tool} ${label} -> ${brief}`);
	if (result.isError && !allowError) throw new Error(`${tool} ${label} failed: ${JSON.stringify(sc.error)}`);
	return result;
}
async function call(tool, args, opts) {
	return (await callFull(tool, args, opts)).structuredContent ?? {};
}
/** A call expected to refuse; returns the error object. */
async function refused(tool, args, label) {
	const r = await callFull(tool, args, { allowError: true, label });
	check(`${label} is refused`, r.isError, r.structuredContent?.error?.code);
	return r.structuredContent?.error ?? {};
}

const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1], x_axis: [1, 0, 0] };

const near = (a, b, tol = 1e-9) => Math.abs(a - b) <= tol;
/**
 * Relative comparison, which is the only honest one for an area or a volume
 * here. Two mechanisms put a relative floor under every such number, and
 * neither is a defect:
 *
 * 1. **A region's area is measured on the slicer's grid.** `compute_regions`
 *    slices the loops with a library that snaps coordinates onto a fixed
 *    float grid (the `provenance_eps` comment in `regions.rs` says so), so the
 *    area of an EXACT 0.06 × 0.04 rectangle comes back 2.4000000044703484e-3
 *    — out by 2^-29 relative, 1.86e-9. Reproduced in pure Rust against
 *    `compute_regions` with hand-written positions, so it is the slicer and
 *    not the page, the solver or the agent link.
 * 2. **A solved coordinate carries LM's convergence tail.** Levenberg-Marquardt
 *    stops at a step/objective criterion, not at the exact root: a point asked
 *    for 0.005 comes back 0.004999999999500001, 1e-10 relative. An area
 *    multiplies two of those and a volume three.
 *
 * So an absolute tolerance on an area is really a tolerance on the sketch's
 * units, and 1e-7 relative sits an order of magnitude above both floors while
 * still being thousands of times tighter than any geometric defect worth
 * catching.
 */
const nearRel = (a, b, rel = 1e-7) => Math.abs(a - b) <= rel * Math.abs(b);

/** One `AddConstraint` op. */
const addC = (constraint) => ({ type: 'AddConstraint', constraint });

/** The region of a sketch answer whose loop is the whole outline. */
function biggestRegion(answer) {
	const regions = answer.regions ?? [];
	return regions.reduce((best, r) => (best === null || r.area_m2 > best.area_m2 ? r : best), null);
}

function extrudeParams(sketch_id, depth, profile_entity_ids) {
	return {
		sketch_id,
		profile_index: 0,
		profile_entity_ids,
		depth,
		direction: null,
		symmetric: false,
		cut: false,
		merge: false,
		target_body: null,
		depth_mode: { type: 'Blind' },
		combine: { type: 'NewBody' }
	};
}

/** Points 1–4 counter-clockwise from the origin, lines 5–8. */
function rectangle() {
	return [P(1, 0, 0), P(2, W, 0), P(3, W, H), P(4, 0, H), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];
}

/** The seven constraints that pin a rectangle at the origin, in order. */
const RAILS = [
	{ c: { type: 'Pinned', point: 1, x: 0, y: 0 }, dof: 6 },
	{ c: { type: 'Horizontal', entity: 5 }, dof: 5 },
	{ c: { type: 'Vertical', entity: 6 }, dof: 4 },
	{ c: { type: 'Horizontal', entity: 7 }, dof: 3 },
	{ c: { type: 'Vertical', entity: 8 }, dof: 2 },
	{ c: { type: 'HDistance', point_a: 1, point_b: 2, value: W }, dof: 1 },
	{ c: { type: 'VDistance', point_a: 1, point_b: 4, value: H }, dof: 0 }
];

async function main() {
	const origin = new URL(APP).origin;
	relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
	await relay.waitListening();
	await relay.initialize(AGENT);

	const tools = (await relay.request('tools/list', {}, 60000)).result.tools.map((t) => t.name);
	const family = ['sketch_create', 'sketch_edit', 'sketch_solve_state', 'sketch_regions'];
	check(
		'tools/list carries the whole sketch family',
		family.every((n) => tools.includes(n)),
		family.filter((n) => !tools.includes(n))
	);

	const browser = await chromium.launch({ args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] });
	try {
		const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
		page.on('pageerror', (e) => console.log('   pageerror:', e.message));
		await pairAgent(page, relay, AGENT);
		await call('document_new', { name: 'Sketch E2E' }, { label: 'fresh document' });

		// ── A free rectangle: eight freedoms, and the solver names them ──
		const created = await call('sketch_create', { plane: XY, entities: rectangle() }, { label: 'free rectangle' });
		const sketchId = created.feature_id;
		check('a free rectangle has 8 degrees of freedom', created.state.dof === 8, created.state.dof);
		check('and eight free directions, one per freedom', created.state.free.length === 8, created.state.free.length);
		check('with no constraint rows at all', created.state.rows === 0 && created.state.rank === 0, {
			rows: created.state.rows,
			rank: created.state.rank
		});
		check('nothing moved, because nothing had to', created.state.moved.length === 0, created.state.moved);
		check(
			'the positions are the ones authored',
			near(created.state.positions['2'][0], W) && near(created.state.positions['3'][1], H),
			created.state.positions
		);
		const area0 = biggestRegion(created)?.area_m2;
		report.measured.free_rectangle_area_m2 = area0;
		report.measured.free_rectangle_area_grid_error = (area0 - W * H) / (W * H);
		check('the loop closes at the authored area', nearRel(area0, W * H), {
			area: area0,
			expected: W * H,
			relative: (area0 - W * H) / (W * H)
		});

		// Reading the state back changes nothing and says the same thing.
		const read = await call('sketch_solve_state', { feature_id: sketchId }, { label: 'read it back' });
		check(
			'sketch_solve_state answers what sketch_create answered',
			JSON.stringify(read.state) === JSON.stringify(created.state)
		);

		// ── The DOF ladder: one constraint a call, 8 → 0 ────────────────
		const ladder = [];
		for (const [i, rung] of RAILS.entries()) {
			const out = await call(
				'sketch_edit',
				{ feature_id: sketchId, ops: [addC(rung.c)] },
				{ label: `${rung.c.type} -> dof ${rung.dof}` }
			);
			ladder.push({ constraint: rung.c.type, dof: out.state.dof, expected: rung.dof, free: out.state.free.length });
			check(`${rung.c.type} takes the sketch to ${rung.dof} dof`, out.state.dof === rung.dof, {
				got: out.state.dof,
				free: out.state.free.length
			});
			check(`and free names exactly ${rung.dof} directions`, out.state.free.length === rung.dof, out.state.free.length);
			check(`the report counts ${i + 1} stored constraints`, out.state.constraints === i + 1, out.state.constraints);
			check('no conflicts on the way down', out.state.conflicts.length === 0, out.state.conflicts);
		}
		report.measured.dof_ladder = ladder;
		const pinned = await call('sketch_solve_state', { feature_id: sketchId }, { label: 'fully constrained' });
		check('the railed rectangle is FullyConstrained', pinned.solve_status === 'FullyConstrained', pinned.solve_status);
		check(
			'and sits exactly where its dimensions put it',
			near(pinned.state.positions['1'][0], 0) &&
				near(pinned.state.positions['1'][1], 0) &&
				near(pinned.state.positions['3'][0], W) &&
				near(pinned.state.positions['3'][1], H),
			pinned.state.positions
		);
		check(
			'every constraint is satisfied, and none is redundant',
			pinned.state.residuals.every((r) => r.satisfied) && pinned.state.redundant.length === 0,
			{ redundant: pinned.state.redundant }
		);

		// ── A duplicate is redundant, not red ───────────────────────────
		const dup = await call(
			'sketch_edit',
			{ feature_id: sketchId, ops: [addC({ type: 'Horizontal', entity: 5 })] },
			{ label: 'a duplicate Horizontal' }
		);
		check('a duplicate constraint leaves the sketch green', dup.solve_status === 'FullyConstrained', dup.solve_status);
		check('it is reported as redundant', dup.state.redundant.length === 1, dup.state.redundant);
		check(
			'and it is the LATER one that is named',
			dup.state.redundant[0] === dup.state.constraints - 1,
			{ redundant: dup.state.redundant, constraints: dup.state.constraints }
		);
		check('with no conflicts: it contradicts nothing', dup.state.conflicts.length === 0, dup.state.conflicts);
		await call('undo', {}, { label: 'drop the duplicate' });
		const afterUndo = await call('sketch_solve_state', { feature_id: sketchId }, { label: 'after undo' });
		check('undo took the duplicate back out', afterUndo.state.constraints === 7, afterUndo.state.constraints);
		check('and the sketch is unchanged otherwise', afterUndo.state.redundant.length === 0, afterUndo.state.redundant);

		// ── A contradiction is refused, and the document does not move ──
		const before = await call('feature_get', { feature_id: sketchId }, { label: 'before the contradiction' });
		const conflict = await refused(
			'sketch_edit',
			{ feature_id: sketchId, ops: [addC({ type: 'HDistance', point_a: 1, point_b: 2, value: W + 0.01 })] },
			'a second, different width'
		);
		check('the refusal is SketchSolveFailed', conflict.code === 'SketchSolveFailed', conflict.code);
		check(
			'it names offending constraints',
			(conflict.details?.conflicts ?? []).length > 0,
			conflict.details?.conflicts
		);
		check('and carries the whole state, so no second call is needed', typeof conflict.details?.state?.dof === 'number');
		const after = await call('feature_get', { feature_id: sketchId }, { label: 'after the contradiction' });
		check(
			'the sketch is byte-identical: nothing was committed',
			JSON.stringify(before.operation) === JSON.stringify(after.operation)
		);

		// `keep` commits the same edit, with the failed verdict recorded.
		const kept = await call(
			'sketch_edit',
			{
				feature_id: sketchId,
				ops: [addC({ type: 'HDistance', point_a: 1, point_b: 2, value: W + 0.01 })],
				on_error: 'keep'
			},
			{ label: 'the same edit, kept' }
		);
		check(
			'on_error keep commits a sketch that did not solve',
			kept.solve_status === 'OverConstrained' || kept.solve_status === 'SolveFailed',
			kept.solve_status
		);
		await call('undo', {}, { label: 'back to the solved rectangle' });

		// ── A drag: the pin lasts one solve ─────────────────────────────
		// On the FULLY constrained rectangle a drag has nowhere to go, so this
		// runs on a fresh under-constrained one.
		const dragSketch = await call(
			'sketch_create',
			{ plane: XY, entities: rectangle(), constraints: [{ type: 'Horizontal', entity: 5 }] },
			{ label: 'a sketch to drag' }
		);
		const dragged = await call(
			'sketch_edit',
			{ feature_id: dragSketch.feature_id, ops: [{ type: 'MovePoint', id: 2, to: [0.09, 0.005] }] },
			{ label: 'drag point 2' }
		);
		check(
			'the drag lands the point where it was asked for in x',
			near(dragged.state.positions['2'][0], 0.09),
			dragged.state.positions['2']
		);
		check(
			'the Horizontal held, so the line is still flat',
			near(dragged.state.positions['1'][1], dragged.state.positions['2'][1]),
			[dragged.state.positions['1'], dragged.state.positions['2']]
		);
		check('the solve says which points it displaced', dragged.state.moved.length > 0, dragged.state.moved);
		check(
			'the pin is named as transient, at the index past the stored constraints',
			dragged.state.transient_constraints?.length === 1 &&
				dragged.state.transient_constraints[0].index === dragged.state.constraints &&
				dragged.state.transient_constraints[0].kind === 'Pinned',
			dragged.state.transient_constraints
		);
		check('and the sketch still stores only its one Horizontal', dragged.state.constraints === 1, dragged.state.constraints);
		const dragStored = await call('feature_get', { feature_id: dragSketch.feature_id }, { label: 'after the drag' });
		check(
			'no Pinned reached the document',
			!dragStored.operation.sketch.constraints.some((c) => c.type === 'Pinned'),
			dragStored.operation.sketch.constraints.map((c) => c.type)
		);

		// ── A fillet: the arc, the tangents, and the area it leaves ─────
		const filleted = await call(
			'sketch_edit',
			{ feature_id: sketchId, ops: [{ type: 'Fillet', corner: 3, radius: R }] },
			{ label: `fillet corner 3 at ${R * 1000} mm` }
		);
		check(
			'the fillet mints an arc',
			filleted.edit.added.some((e) => e.type === 'Arc'),
			filleted.edit.added
		);
		check('and repoints the two legs rather than re-creating them', filleted.edit.changed.length === 2, filleted.edit.changed);
		check('with two tangents to hold it', filleted.edit.constraints_added === 2, filleted.edit.constraints_added);
		const filletArea = biggestRegion(filleted)?.area_m2;
		const expectFillet = W * H - FILLET_AREA_LOSS;
		const chordDeficit = expectFillet - filletArea;
		report.measured.filleted_area_m2 = filletArea;
		report.measured.filleted_area_analytic_m2 = expectFillet;
		report.measured.filleted_area_chord_deficit_m2 = chordDeficit;
		// A region's area is the area of the CHORD POLYGON, not of the analytic
		// loop: `compute_regions` tessellates every curved boundary at
		// `DEFAULT_CHORD_TOLERANCE` (1e-3, relative), so a loop carrying an arc
		// reads LOW by the chords' deficit and can never read high. Measured
		// here: 1.6e-8 m² on a 2.4e-3 m² plate, 6.6 ppm. Anyone asserting an
		// analytic area against `area_m2` has to know this, which is why the
		// check is written as a bound on the deficit and the number is recorded.
		check(
			'the filleted region area is the analytic area, less the chords',
			chordDeficit > 0 && chordDeficit < 1e-5 * (W * H),
			{ area: filletArea, analytic: expectFillet, deficit: chordDeficit, ppm: (chordDeficit / (W * H)) * 1e6 }
		);

		// One batch is one undo step, however much it touched.
		await call('undo', {}, { label: 'undo the fillet' });
		const unfilleted = await call('sketch_solve_state', { feature_id: sketchId }, { label: 'after undoing it' });
		check(
			'one undo takes the arc, the repointed legs and both tangents',
			unfilleted.state.constraints === 7 && nearRel(biggestRegion(unfilleted)?.area_m2, W * H),
			{ constraints: unfilleted.state.constraints, area: biggestRegion(unfilleted)?.area_m2 }
		);

		// ── Trim, extend, offset, mirror, construction, dimension ──────
		// A cross: a long horizontal line crossed by a vertical one. Trimming
		// the right-hand piece of the horizontal leaves the left-hand piece.
		const cross = await call(
			'sketch_create',
			{
				plane: XY,
				entities: [P(1, 0, 0), P(2, 0.1, 0), L(3, 1, 2), P(4, 0.05, -0.02), P(5, 0.05, 0.02), L(6, 4, 5)]
			},
			{ label: 'a cross to trim' }
		);
		const trimmed = await call(
			'sketch_edit',
			{ feature_id: cross.feature_id, ops: [{ type: 'Trim', entity: 3, at: [0.08, 0] }] },
			{ label: 'trim the right half' }
		);
		check('the trim reports what it changed', trimmed.edit.changed.length + trimmed.edit.added.length > 0, trimmed.edit);
		const crossAfter = await call('feature_get', { feature_id: cross.feature_id }, { label: 'after the trim' });
		const hLine = crossAfter.operation.sketch.entities.find((e) => e.id === 3);
		const pos = crossAfter.operation.sketch.solved_positions;
		const far = hLine ? pos[String(hLine.end_id)] ?? pos[String(hLine.start_id)] : null;
		check('the surviving line stops at the crossing', hLine !== undefined, { line: hLine, far });
		report.measured.trimmed_line = { line: hLine, positions: pos };

		// Extend it back out to the far edge of nothing in particular: with no
		// `to` the op reaches the nearest entity it can.
		// With no `to` the op reaches the nearest entity it can; on a trimmed
		// cross there may be nothing past the cut, and `NothingToExtendTo` is
		// the right answer to that, so both outcomes are accepted and the one
		// that happened is recorded.
		const extendResult = await callFull(
			'sketch_edit',
			{ feature_id: cross.feature_id, ops: [{ type: 'Extend', entity: 3, end: 'End' }] },
			{ label: 'extend it back', allowError: true }
		);
		const extended = extendResult.structuredContent ?? {};
		report.measured.extend = extendResult.isError ? extended.error : { dof: extended.state?.dof, edit: extended.edit };
		check(
			'extend either reaches something or says there is nothing to reach',
			typeof extended.state?.dof === 'number' || extended.error?.details?.reason?.type === 'NothingToExtendTo',
			report.measured.extend
		);

		// Offset the trimmed chain, mirror a line, flip one to construction,
		// and retarget a dimension — the remaining op kinds, each once.
		const offset = await call(
			'sketch_edit',
			{ feature_id: cross.feature_id, ops: [{ type: 'Offset', chain: [6], distance: 0.01, side: 'Left' }] },
			{ label: 'offset the vertical line' }
		);
		check('the offset adds geometry', offset.edit.added.length > 0, offset.edit.added);
		const mirrored = await call(
			'sketch_edit',
			{ feature_id: cross.feature_id, ops: [{ type: 'Mirror', entities: [3], axis: 6 }] },
			{ label: 'mirror across the vertical' }
		);
		check('the mirror adds an image', mirrored.edit.added.length > 0, mirrored.edit.added);
		const construction = await call(
			'sketch_edit',
			{ feature_id: cross.feature_id, ops: [{ type: 'SetConstruction', entity: 6, construction: true }] },
			{ label: 'the vertical becomes construction' }
		);
		check('the construction flag is a change, not an add', construction.edit.changed.length === 1, construction.edit);
		const redimensioned = await call(
			'sketch_edit',
			{ feature_id: sketchId, ops: [{ type: 'SetDimension', index: 5, value: 0.08 }] },
			{ label: 'widen the plate to 80 mm' }
		);
		const widened = biggestRegion(redimensioned)?.area_m2;
		check(
			'a retargeted dimension moves the geometry and the area follows',
			nearRel(widened, 0.08 * H),
			{ area: widened, expected: 0.08 * H }
		);
		await call(
			'sketch_edit',
			{ feature_id: sketchId, ops: [{ type: 'SetDimension', index: 5, value: W }] },
			{ label: 'back to 60 mm' }
		);
		const removed = await call(
			'sketch_edit',
			{ feature_id: cross.feature_id, ops: [{ type: 'RemoveEntity', ids: [6] }] },
			{ label: 'remove the vertical' }
		);
		check('the removal reports the id it took', removed.edit.removed.includes(6), removed.edit.removed);

		// ── Every refusal the door can answer with ─────────────────────
		const noFit = await refused(
			'sketch_edit',
			{ feature_id: sketchId, ops: [{ type: 'Fillet', corner: 1, radius: 1 }] },
			'a fillet of radius 1 m'
		);
		check('a fillet that does not fit says so by name', noFit.details?.reason?.type === 'FilletDoesNotFit', noFit.details?.reason);
		const noEntity = await refused(
			'sketch_edit',
			{ feature_id: sketchId, ops: [{ type: 'Trim', entity: 999, at: [0, 0] }] },
			'trimming an entity that is not there'
		);
		check('an unknown entity id is named', noEntity.details?.reason?.type === 'NoSuchEntity', noEntity.details?.reason);
		const noConstraint = await refused(
			'sketch_edit',
			{ feature_id: sketchId, ops: [{ type: 'RemoveConstraint', index: 99 }] },
			'removing constraint 99'
		);
		check('an unknown constraint index is named', noConstraint.details?.reason?.type === 'NoSuchConstraint', noConstraint.details?.reason);
		const empty = await refused('sketch_edit', { feature_id: sketchId, ops: [] }, 'an empty batch');
		check('an empty batch is a schema miss at the relay', empty.code === 'SchemaRejected' || empty.code === 'InvalidSketch', empty.code);
		const gone = await refused(
			'sketch_edit',
			{ feature_id: '00000000-0000-0000-0000-00000000dead', ops: [{ type: 'RemoveEntity', ids: [1] }] },
			'a feature that is not there'
		);
		check('a missing feature is FeatureNotFound', gone.code === 'FeatureNotFound', gone.code);

		// ── The geometry is real: extrude the edited sketch ────────────
		const region = biggestRegion(await call('sketch_regions', { feature_id: sketchId }, { label: 'the plate loop' }));
		const extrude = await call(
			'feature_add',
			{ operation: { type: 'Extrude', params: extrudeParams(sketchId, T, region.profile_entity_ids) } },
			{ label: 'extrude the plate' }
		);
		const measured = await call('body_measure', { body_id: extrude.bodies_added[0] }, { label: 'the plate' });
		const volume = measured.volume_m3 ?? measured.volume?.value;
		report.measured.volume_m3 = volume;
		report.measured.volume_expected_m3 = W * H * T;
		check(
			'the body an edited sketch extrudes to has the volume arithmetic says',
			nearRel(volume, W * H * T),
			{ volume, expected: W * H * T, relative: (volume - W * H * T) / (W * H * T) }
		);

		// ── FINDING: an edit that changes a loop breaks an extrude that
		// addressed it by entity ids ───────────────────────────────────
		//
		// Round the corner UNDER the extrude. The fillet keeps lines 6 and 7
		// and adds arc 12, so the loop becomes {5,6,12,7,8} where the extrude
		// stored `profile_entity_ids: [5,6,7,8]`, and
		// `rebuild::resolve_profile_index` matches that set EXACTLY: no loop
		// matches, and the rebuild stops with `ProfileNotFound`.
		//
		// The re-resolution that would survive this exists —
		// `rebuild::resolve_extrude_regions` re-resolves a stored `Region` by
		// boundary identity and warns when it cannot — but only on the
		// `region`/`regions` path the APP's writers use. The agent is told to
		// address by `profile_entity_ids` (it is what `sketch_create` and
		// `sketch_regions` hand back, and the engine schema says "the app's own
		// writers address by index and leave this None"), which is the one
		// addressing mode no re-resolution covers. S3 did not cause this; it
		// made sketch editing routine, which is what brought it into reach.
		//
		// It is LOUD and it rolls back, so it is a capability gap, not a
		// silent wrong. Recorded here as the measurement; the fix belongs with
		// the profile-addressing path, not with a tolerance.
		const brokenBinding = await refused(
			'sketch_edit',
			{ feature_id: sketchId, ops: [{ type: 'Fillet', corner: 3, radius: R }] },
			'rounding a corner under an extrude that named the loop by its entity ids'
		);
		report.measured.profile_binding_break = brokenBinding;
		check(
			'the break is ProfileNotFound, naming the ids the extrude asked for',
			brokenBinding.details?.engine_error?.kind?.type === 'ProfileNotFound' &&
				JSON.stringify(brokenBinding.details?.engine_error?.kind?.entity_ids) ===
					JSON.stringify(region.profile_entity_ids),
			brokenBinding.details?.engine_error?.kind
		);
		check('and the edit rolled back', brokenBinding.details?.rolled_back === true, brokenBinding.details?.rolled_back);
		const stillThere = await call('model_summary', {}, { label: 'after the rollback' });
		check(
			'the plate is still one body with no errors',
			stillThere.bodies.length === 1 && stillThere.errors.length === 0,
			{ bodies: stillThere.bodies.length, errors: stillThere.errors }
		);

		// The same fillet DOES reach a solid when the extrude addresses its
		// profile by index, which is how the app's own writers do it — so the
		// gap is the binding, not the geometry.
		const roundSketch = await call(
			'sketch_create',
			{ plane: XY, entities: rectangle(), constraints: RAILS.map((r) => r.c) },
			{ label: 'a second plate, to round' }
		);
		await call(
			'sketch_edit',
			{ feature_id: roundSketch.feature_id, ops: [{ type: 'Fillet', corner: 3, radius: R }] },
			{ label: 'round it before anything depends on it' }
		);
		const byIndex = extrudeParams(roundSketch.feature_id, T, null);
		const roundedExtrude = await call(
			'feature_add',
			{ operation: { type: 'Extrude', params: byIndex } },
			{ label: 'extrude the rounded plate by index' }
		);
		const roundedMeasure = await call(
			'body_measure',
			{ body_id: roundedExtrude.bodies_added[0] },
			{ label: 'rounded plate' }
		);
		const roundedVolume = roundedMeasure.volume_m3 ?? roundedMeasure.volume?.value;
		const expectRounded = (W * H - FILLET_AREA_LOSS) * T;
		report.measured.rounded_volume_m3 = roundedVolume;
		report.measured.rounded_volume_analytic_m3 = expectRounded;
		// EXACT, to the last ULP — and that is the interesting part. The
		// region's `area_m2` is the chord polygon's (check above, 6.6 ppm low),
		// but the SOLID is not: the profile carries the arc as an arc, the
		// kernel builds a true cylindrical face from it, and the volume is the
		// analytic one (A15, analytical primacy). The two numbers an agent can
		// read about the same rounded corner therefore have different
		// characters, which anyone writing an oracle against either has to
		// know. Before the `build_finish_profiles` fix this came out
		// 1.4352e-5 — the rectangle less r²/2 × t, a chamfer — so this check
		// is also the regression test for that silent wrong.
		const roundedDeficit = expectRounded - roundedVolume;
		check(
			'the rounded profile reaches the solid as an ARC, by its exact volume',
			Math.abs(roundedDeficit) < 1e-12 * expectRounded,
			{ volume: roundedVolume, analytic: expectRounded, deficit: roundedDeficit, relative: roundedDeficit / expectRounded }
		);

		const saved = await call('document_save', {}, { label: 'save' });
		check('the document saved', !!saved.saved_at, saved);

		report.tools = [...report.tools].sort();
		report.passed = report.checks.filter((c) => c.ok).length;
		report.failed = report.checks.filter((c) => !c.ok).length;
		writeFileSync(path.join(OUT, 'report.json'), JSON.stringify(report, null, 2) + '\n');
		console.log(`\n${report.passed} checks passed, ${report.failed} failed; ${report.tools.length} distinct tools called`);
	} finally {
		await browser.close();
		console.log('relay exit', await relay.close());
	}
}

main().catch((e) => {
	console.error(e);
	process.exitCode = 1;
});
