/**
 * Exercise EVERY drawing tool through the real agent link: a local
 * `waffle-mcp-relay` driven as an MCP client over stdio, paired with a
 * headless page on the running dev server (the harness the agent-*.spec.js
 * files use), on a document authored through the same link.
 *
 * What it drives, in order (specs/drawings_and_mbd.md §8 D4a–D4f):
 *   document_new → sketch_create + feature_add (a 60 × 40 × 6 mm plate, Ø10 bore)
 *   tab_add Drawing → drawing_view_add top (include_anchors)
 *   drawing_annotation_add: Distance (two walls), Diameter (the bore), Note
 *   drawing_get (include_annotations) — the measured values
 *   drawing_annotation_edit (precision, dual unit, placement; then a refused field)
 *   drawing_annotation_delete (the note)
 *   drawing_view_add projected right, section A-A, detail B
 *   drawing_view_delete (the projected view; the section and detail cascade)
 *   drawing_sheet_edit (title block rows incl. an expression row)
 *   export_svg, export_pdf, export_dxf (sheet), export_dxf iso (part tab)
 *   document_save
 *
 * Output (all under OUT, default ./out beside this file):
 *   calls.jsonl           every tool call and its structuredContent
 *   plate.svg / plate.pdf / plate-sheet.dxf / plate-iso.dxf
 *   report.json           the measured numbers the README quotes
 *
 * Run from the repo root with the dev server up:
 *   node docs/notes/drawings_mcp_e2e/drawings_e2e.mjs
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
const AGENT = 'claude-code-drawings-e2e';
const CALL_TIMEOUT_MS = 300_000;

// The plate, in meters.
const PW = 0.060;
const PD = 0.040;
const PT = 0.006;
const BORE_R = 0.005;
const PLATE_VOLUME_EXPECTED = PW * PD * PT - Math.PI * BORE_R * BORE_R * PT;

let relay;
const t0 = Date.now();
const report = { checks: [], tools: new Set() };

function log(entry) {
	appendFileSync(LOG, JSON.stringify(entry) + '\n');
}

function check(name, ok, detail) {
	report.checks.push({ name, ok: !!ok, detail });
	console.log(`   ${ok ? 'PASS' : 'FAIL'} ${name}${detail === undefined ? '' : ' — ' + JSON.stringify(detail)}`);
	if (!ok) process.exitCode = 1;
}

/** One tool call; throws on isError unless `allowError`. Returns the full result. */
async function callFull(tool, args = {}, { allowError = false, label = '' } = {}) {
	const started = Date.now();
	let result;
	try {
		result = await relay.callTool(tool, args, CALL_TIMEOUT_MS);
	} catch (e) {
		// The relay validates arguments against the manifest's inputSchema
		// BEFORE the page sees the call (spec §2.4): a schema miss is a
		// JSON-RPC -32602 with the JSON pointer, not a tool result. For a
		// call expected to refuse, that is the refusal.
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
const C = (id, center_id, radius) => ({ type: 'Circle', id, center_id, radius });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1], x_axis: [1, 0, 0] };

function solidRef(feature_id) {
	return {
		kind: { type: 'Solid' },
		anchor: { type: 'FeatureOutput', feature_id, output_key: { type: 'Main' } },
		selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
		policy: { type: 'Strict' }
	};
}

function extrudeParams(sketch_id, depth, { combine = 'NewBody', targets = null } = {}) {
	const p = {
		sketch_id,
		profile_index: 0,
		depth,
		direction: null,
		symmetric: false,
		cut: combine === 'Cut',
		merge: false,
		target_body: null,
		depth_mode: { type: 'Blind' },
		combine: { type: combine }
	};
	p.targets = targets ?? [];
	return p;
}

/** Save an export result's embedded resource to OUT. */
function saveExport(result, fileName) {
	const res = result.content?.find((c) => c.type === 'resource')?.resource;
	if (!res) throw new Error(`no embedded resource in ${fileName}`);
	const file = path.join(OUT, fileName);
	if (res.text !== undefined) writeFileSync(file, res.text);
	else writeFileSync(file, Buffer.from(res.blob, 'base64'));
	console.log(`   wrote ${file} (${result.structuredContent.bytes} bytes)`);
	return res.text ?? null;
}

const near = (a, b, tol = 1e-9) => Math.abs(a - b) <= tol;

// ---------------------------------------------------------------- the part
async function buildPlate() {
	const s = await call(
		'sketch_create',
		{
			plane: XY,
			entities: [P(1, 0, 0), P(2, PW, 0), P(3, PW, PD), P(4, 0, PD), L(10, 1, 2), L(11, 2, 3), L(12, 3, 4), L(13, 4, 1)]
		},
		{ label: 'plate sketch' }
	);
	const e = await call(
		'feature_add',
		{ operation: { type: 'Extrude', params: extrudeParams(s.feature_id, PT) } },
		{ label: 'plate extrude' }
	);
	await call('feature_rename', { feature_id: e.feature_id, new_name: 'Plate' }, { label: 'Plate' });
	const m = await call('body_measure', { body_id: e.bodies_added[0] }, { label: 'plate measure' });
	const dir = Math.abs(m.bbox_max[2]) > Math.abs(m.bbox_min[2]) ? 1 : -1;
	// The bore: a cylinder tool through the plate's mid-plane, extruded
	// SYMMETRICALLY so it spans the plate whichever way a blind cut would
	// have gone (a blind cut with no direction auto-reverses — a first run
	// of this script cut nothing, silently: volume unchanged, no error).
	const bs = await call(
		'sketch_create',
		// `x_axis` pinned to world +x: a plane given as origin + normal gets
		// engine-derived in-plane axes (here +x along world −y), which put the
		// first run's circle outside the plate — a disjoint cut removes
		// nothing and says nothing (the trap `sketch_create`'s answer `plane`
		// exists to expose).
		{
			plane: { origin: [0, 0, (dir * PT) / 2], normal: [0, 0, 1], x_axis: [1, 0, 0] },
			entities: [P(1, PW / 2, PD / 2), C(2, 1, BORE_R)]
		},
		{ label: 'bore sketch' }
	);
	const cutParams = extrudeParams(bs.feature_id, PT + 0.004, { combine: 'Cut', targets: [solidRef(e.feature_id)] });
	cutParams.symmetric = true;
	const cut = await call(
		'feature_add',
		{ operation: { type: 'Extrude', params: cutParams } },
		{ label: 'bore cut' }
	);
	await call('feature_rename', { feature_id: cut.feature_id, new_name: 'Bore' }, { label: 'Bore' });
	const summary = await call('model_summary', {}, { label: 'after bore' });
	check('the plate is one body with no errors', summary.bodies.length === 1 && summary.errors.length === 0, {
		bodies: summary.bodies.length,
		errors: summary.errors
	});
	// The body is NAMED, so a title-block expression can measure it:
	// `volume(Plate)` resolves a body or entity name, not a feature name.
	await call('body_rename', { body_id: summary.bodies[0].body_id, new_name: 'Plate' }, { label: 'name the body' });
	const measured = await call('body_measure', { body_id: summary.bodies[0].body_id }, { label: 'bored plate' });
	const expectVol = PLATE_VOLUME_EXPECTED;
	check('the bored plate volume is the closed form', near(measured.volume_m3, expectVol, 1e-12), {
		volume_m3: measured.volume_m3,
		expected: expectVol
	});
	return summary;
}

// ---------------------------------------------------------------- anchors
/** Two parallel walls of the top view, by extreme u, and one bore rim. */
function pickAnchors(view) {
	// `shape` and `kind` ride as `{type}` objects (serde's tagged enums).
	const list = view.anchor_list ?? [];
	const lines = list.filter((a) => a.shape?.type === 'Line' && Array.isArray(a.at));
	const us = lines.map((a) => a.at[0]);
	const minU = Math.min(...us);
	const maxU = Math.max(...us);
	const left = lines.find((a) => near(a.at[0], minU, 1e-9));
	const right = lines.find((a) => near(a.at[0], maxU, 1e-9));
	const circle = list.find((a) => a.shape?.type === 'Circle' && a.kind?.type === 'Edge');
	return { left, right, circle, count: list.length, span: maxU - minU };
}

// ---------------------------------------------------------------- main
async function main() {
	const origin = new URL(APP).origin;
	relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
	await relay.waitListening();
	await relay.initialize(AGENT);

	// The manifest the relay serves must carry the whole family.
	const tools = (await relay.request('tools/list', {}, 60000)).result.tools.map((t) => t.name);
	const family = [
		'drawing_get',
		'drawing_view_add',
		'drawing_view_edit',
		'drawing_view_delete',
		'drawing_annotation_add',
		'drawing_annotation_edit',
		'drawing_annotation_delete',
		'drawing_sheet_edit',
		'export_dxf',
		'export_svg',
		'export_pdf'
	];
	check(
		'tools/list carries the whole drawing family',
		family.every((n) => tools.includes(n)),
		family.filter((n) => !tools.includes(n))
	);

	const browser = await chromium.launch({ args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] });
	try {
		const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
		page.on('pageerror', (e) => console.log('   pageerror:', e.message));
		await pairAgent(page, relay, AGENT);
		const status = await call('waffle_status', {}, { label: 'paired' });
		check('the page is paired and ready', status.state === 'ready', status);

		const doc = await call('document_new', { name: 'Drawings E2E plate' }, { label: 'new document' });
		const partTab = doc.active_tab ?? doc.tabs?.[0]?.id;
		await buildPlate();

		// ---- the drawing tab, and the gate
		const err = await refused('drawing_get', {}, 'drawing_get on a Part tab');
		check('the gate names the tab kind', err.code === 'TabKindNotSupported' && err.details?.kind === 'Part', err);

		const tab = await call('tab_add', { kind: 'Drawing', name: 'Plate drawing' }, { label: 'drawing tab' });
		const drawingTab = tab.tab_id;
		const empty = await call('drawing_get', {}, { label: 'empty drawing' });
		check('a fresh drawing has one sheet and no views', empty.sheets.length === 1 && empty.sheets[0].views.length === 0, {
			sheets: empty.sheets.length
		});

		// ---- the top view, with anchors
		const added = await call(
			'drawing_view_add',
			{ tab_id: partTab, view: 'top', scale: 2, placement_mm: [100, 110], include_anchors: true },
			{ label: 'top view 2:1' }
		);
		const topId = added.view_id;
		const topView = added.sheets[0].views.find((v) => v.id === topId);
		check('the top view drew curves and no view errored', topView.curves > 0 && added.errors.length === 0, {
			curves: topView.curves,
			errors: added.errors
		});
		const picked = pickAnchors(topView);
		check('anchors were offered: two walls and a bore rim', !!(picked.left && picked.right && picked.circle), {
			anchors: picked.count,
			span: picked.span
		});
		check('the walls span the plate width or depth', near(picked.span, PW, 1e-9) || near(picked.span, PD, 1e-9), picked.span);
		check('an anchor pid is a decimal string', typeof picked.left.pid === 'string' && /^[0-9]+$/.test(picked.left.pid), picked.left.pid);

		// ---- annotations
		const dist = await call(
			'drawing_annotation_add',
			{ view_id: topId, annotation: 'Dimension', kind: 'Distance', anchors: [picked.left, picked.right], precision: 2 },
			{ label: 'distance between walls' }
		);
		check('the distance landed at index 0', dist.annotation_index === 0);
		const dia = await call(
			'drawing_annotation_add',
			{
				view_id: topId,
				annotation: 'Dimension',
				kind: 'Diameter',
				anchors: [{ pid: picked.circle.pid, kind: picked.circle.kind }],
				precision: 1
			},
			{ label: 'bore diameter' }
		);
		check('the diameter landed at index 1', dia.annotation_index === 1);
		const note = await call(
			'drawing_annotation_add',
			{ view_id: topId, annotation: 'Note', text: 'DEBURR ALL EDGES', anchors: [picked.left.pid] },
			{ label: 'note' }
		);
		check('the note landed at index 2', note.annotation_index === 2);

		// A typed value is refused by name.
		const typed = await refused(
			'drawing_annotation_add',
			{ view_id: topId, annotation: 'Dimension', kind: 'Distance', anchors: [picked.left, picked.right], value: 0.123 },
			'a typed-in value'
		);
		// The relay's schema check refuses `value` first (-32602 with the
		// pointer); the engine's own by-name refusal, which names `expr`, is
		// pinned in crates/wasm-bridge/tests/tool_drawing.rs.
		check('the value refusal is the schema\'s, with the pointer', typed.code === 'SchemaRejected' && /value/.test(typed.message), typed);

		// ---- read back
		const got = await call('drawing_get', { include_annotations: true }, { label: 'read back' });
		const view = got.sheets[0].views.find((v) => v.id === topId);
		const list = view.annotation_list;
		check('drawing_get lists three annotations', list.length === 3, list.map((a) => a.annotation));
		const d0 = list[0];
		check(
			'the distance measured the plate',
			d0.resolved && (near(d0.value, PW, 1e-9) || near(d0.value, PD, 1e-9)),
			{ value: d0.value }
		);
		check('the distance keeps its precision and anchors', d0.precision === 2 && d0.anchors.length === 2, d0);
		const d1 = list[1];
		check('the diameter measured 2r of the bore', d1.resolved && near(d1.value, 2 * BORE_R, 1e-9), { value: d1.value });
		check('the note reads back its text', list[2].annotation === 'Note' && list[2].text === 'DEBURR ALL EDGES');
		// An anchor read off drawing_get passes back verbatim.
		const again = await call(
			'drawing_annotation_add',
			{ view_id: topId, annotation: 'CentreMark', anchors: [d1.anchors[0]] },
			{ label: 'centre mark from a read-back anchor' }
		);
		check('a read-back anchor is accepted verbatim', again.annotation_index === 3);

		// ---- edit in place
		const edited = await call(
			'drawing_annotation_edit',
			{ view_id: topId, index: 0, precision: 3, dual_unit: 'in', placement: [0, 0.012] },
			{ label: 'edit the distance' }
		);
		const e0 = edited.sheets[0].views.find((v) => v.id === topId).annotation_list[0];
		check('the edit kept index 0 and applied every field', e0.annotation === 'Dimension' && e0.precision === 3 && e0.dual_unit === 'in' && near(e0.placement[1], 0.012), e0);
		const wrong = await refused('drawing_annotation_edit', { view_id: topId, index: 2, precision: 1 }, 'precision on a note');
		check('the wrong-field refusal names the field', wrong.code === 'InvalidArgument' && /precision/.test(wrong.message), wrong.message);
		const badExpr = await refused(
			'drawing_annotation_edit',
			{ view_id: topId, index: 0, expr: 'no_such_parameter' },
			'an expression that does not evaluate'
		);
		check('a bad expression is AnnotationNotMeasurable', badExpr.code === 'AnnotationNotMeasurable', badExpr.code);
		const afterBad = await call('drawing_get', { include_annotations: true }, { label: 'after the refused expr' });
		const a0 = afterBad.sheets[0].views.find((v) => v.id === topId).annotation_list[0];
		check('the refused expression was rolled back', a0.expr === null && a0.resolved === true && a0.precision === 3, a0);

		// A parameter-driven expression that DOES evaluate.
		await call('tab_switch', { tab_id: partTab }, { label: 'to the part' });
		await call(
			'parameters_set',
			{ parameters: [{ name: 'plate_w', expression: String(PW * 1000) }] },
			{ label: 'plate_w parameter' }
		);
		await call('tab_switch', { tab_id: drawingTab }, { label: 'back to the drawing' });
		const withExpr = await call(
			'drawing_annotation_edit',
			{ view_id: topId, index: 0, expr: 'plate_w / 2' },
			{ label: 'half-width expression' }
		);
		const x0 = withExpr.sheets[0].views.find((v) => v.id === topId).annotation_list[0];
		check('the expression dimension prints the expression', x0.expr === 'plate_w / 2' && near(x0.value, PW / 2, 1e-9), x0.value);
		await call('drawing_annotation_edit', { view_id: topId, index: 0, expr: '' }, { label: 'back to measuring' });

		// ---- delete an annotation
		const del = await call('drawing_annotation_delete', { view_id: topId, index: 2 }, { label: 'delete the note' });
		const afterDel = await call('drawing_get', { include_annotations: true }, { label: 'after delete' });
		const l2 = afterDel.sheets[0].views.find((v) => v.id === topId).annotation_list;
		check('the note is gone and the centre mark moved up', del.index === 2 && l2.length === 3 && l2[2].annotation === 'CentreMark', l2.map((a) => a.annotation));
		const past = await refused('drawing_annotation_delete', { view_id: topId, index: 7 }, 'an index past the list');
		check('the past-the-end refusal says the count', past.code === 'NotFound' && past.details?.count === 3, past.details);

		// ---- derived views: projected, section, detail
		const side = await call(
			'drawing_view_add',
			{ tab_id: partTab, parent_view_id: topId, direction_from_parent: 'right', scale: 2 },
			{ label: 'projected right' }
		);
		const sideId = side.view_id;
		const sideView = side.sheets[0].views.find((v) => v.id === sideId);
		const sideSpans = sideView.bbox ? [sideView.bbox[1][0] - sideView.bbox[0][0], sideView.bbox[1][1] - sideView.bbox[0][1]] : null;
		check('the projected view shows the plate thickness', !!sideSpans && sideSpans.some((s) => near(s, PT, 1e-9)), sideSpans);
		// A horizontal cut through the TOP view at the bore's centre line, in
		// that view's own (u, v) millimetres — read off its bbox rather than
		// assumed, because the view's origin is the projection's, not the
		// paper's. A first run cut the side view along its own boundary and
		// was told, correctly, that the cut kept no material.
		const [[u0, v0], [u1, v1]] = topView.bbox;
		const vMid = ((v0 + v1) / 2) * 1000;
		const section = await call(
			'drawing_view_add',
			{
				tab_id: partTab,
				parent_view_id: topId,
				section_mm: [u0 * 1000 - 5, vMid, u1 * 1000 + 5, vMid],
				scale: 2,
				label: 'A'
			},
			{ label: 'section A-A through the bore' }
		);
		const sectionView = section.sheets[0].views.find((v) => v.id === section.view_id);
		check('the section has a hatched cap', (sectionView.hatch_loops ?? 0) > 0 && section.errors.length === 0, {
			hatch_loops: sectionView.hatch_loops,
			errors: section.errors
		});
		const detail = await call(
			'drawing_view_add',
			{ tab_id: partTab, parent_view_id: topId, detail_mm: [0, 0, 8], scale: 4, label: 'B' },
			{ label: 'detail B of the bore' }
		);
		check('four views are on the sheet', detail.sheets[0].views.length === 4, detail.sheets[0].views.length);

		// edit a view
		const moved = await call('drawing_view_edit', { view_id: detail.view_id, placement_mm: [240, 60], hidden_lines: false }, { label: 'move the detail' });
		const mv = moved.sheets[0].views.find((v) => v.id === detail.view_id);
		check('the view edit moved the detail and turned hidden lines off', mv.placement_mm[0] === 240 && mv.style.hidden_lines === false, mv);

		// ---- the sheet: title block with an expression row
		const sheet = await call(
			'drawing_sheet_edit',
			{
				size: 'A3',
				orientation: 'landscape',
				projection_angle: 'third',
				title_block: true,
				title_block_fields: [
					{ key: 'DocumentName' },
					{ key: 'SheetNumber' },
					{ key: 'Scale' },
					{ key: 'ProjectionAngle' },
					{ key: 'Author', text: 'claude-code (MCP e2e)' },
					{ key: 'Material', text: '6061-T6' },
					// Expressions evaluate in MILLIMETRE space (volume in mm³), so
					// the density is g/mm³. The printed row keeps the volume's
					// unit suffix — a proper mass() is M1's.
					{ label: 'Mass (g)', expr: 'volume(Plate) * 0.0027' }
				]
			},
			{ label: 'title block' }
		);
		// `title_block.rows` is the FILLED block: `{rows: [{label, value}]}`.
		const rows = sheet.sheets[0].title_block.rows?.rows ?? [];
		const massRow = rows.find((r) => r.label === 'Mass (g)');
		const massG = PLATE_VOLUME_EXPECTED * 1e9 * 0.0027;
		check(
			'the title block prints the expression row as the measured mass',
			!!massRow && near(parseFloat(massRow.value), massG, 0.01),
			{ row: massRow, expected_g: massG, errors: sheet.errors, warnings: sheet.warnings }
		);
		check('the derived rows are filled from the document', rows.some((r) => r.value === '2:1') && rows.some((r) => /Third/.test(r.value)), rows);
		const derivedTyped = await refused(
			'drawing_sheet_edit',
			{ title_block_fields: [{ key: 'SheetNumber', text: '7' }] },
			'typing a derived title-block row'
		);
		check('the derived-row refusal names the row', derivedTyped.code === 'InvalidArgument' && /SheetNumber/.test(derivedTyped.message), derivedTyped.message);

		// ---- exports from the sheet
		const svg = await callFull('export_svg', { deliver: 'agent' }, { label: 'sheet SVG' });
		const svgText = saveExport(svg, 'plate.svg');
		check('the SVG carries the printed width', /60\.000|40\.000/.test(svgText), svgText.match(/\d+\.\d{3}/g)?.slice(0, 4));
		check('the SVG carries the bore diameter', /10\.0/.test(svgText));
		const pdf = await callFull('export_pdf', { deliver: 'agent' }, { label: 'sheet PDF' });
		saveExport(pdf, 'plate.pdf');
		check('the PDF is one page', pdf.structuredContent.pages === 1, pdf.structuredContent);
		const dxf = await callFull('export_dxf', { deliver: 'agent' }, { label: 'sheet DXF' });
		const dxfText = saveExport(dxf, 'plate-sheet.dxf');
		check('the sheet DXF carries the HATCH layer', /\nHATCH\n/.test(dxfText));
		const dxfModelArgs = await refused('export_dxf', { view: 'top' }, 'a model-view argument on the drawing tab');
		check('the model-view argument is refused on a drawing tab', dxfModelArgs.code === 'InvalidArgument');

		// ---- delete views: a leaf alone, then a parent with its cascade
		const vdel = await call('drawing_view_delete', { view_id: sideId }, { label: 'delete the side view' });
		check('deleting the side view took only itself', vdel.deleted.length === 1 && vdel.deleted[0] === sideId, vdel.deleted);
		check('three views remain', vdel.sheets[0].views.length === 3, vdel.sheets[0].views.length);
		const gone = await refused('drawing_view_delete', { view_id: sideId }, 'deleting it again');
		check('a second delete is NotFound', gone.code === 'NotFound');
		const tdel = await call('drawing_view_delete', { view_id: topId }, { label: 'delete the top view' });
		const tookAll = [topId, section.view_id, detail.view_id].every((id) => tdel.deleted.includes(id));
		check('deleting the top view took its section and detail and said so', tdel.deleted.length === 3 && tookAll, tdel.deleted);
		check('no view remains', tdel.sheets[0].views.length === 0, tdel.sheets[0].views.length);

		// ---- the one-view iso export, on the part tab
		await call('tab_switch', { tab_id: partTab }, { label: 'to the part for the iso DXF' });
		const iso = await callFull('export_dxf', { view: 'iso', deliver: 'agent' }, { label: 'iso DXF' });
		const isoText = saveExport(iso, 'plate-iso.dxf');
		const isoLines = (isoText.match(/\nLINE\n/g) ?? []).length;
		const isoCircles = (isoText.match(/\nCIRCLE\n|\nELLIPSE\n|\nPOLYLINE\n/g) ?? []).length;
		check('the iso DXF has the plate edges and the bore rims', isoLines >= 9 && isoCircles >= 1, { lines: isoLines, curved: isoCircles });

		await call('tab_switch', { tab_id: drawingTab }, { label: 'back to the drawing' });
		const saved = await call('document_save', {}, { label: 'save' });
		check('the document saved', !!saved.saved_at, saved);

		// Undo reaches the drawing's own stack while the drawing tab is active.
		const undone = await call('undo', {}, { label: 'undo the view delete' });
		const afterUndo = await call('drawing_get', {}, { label: 'after undo' });
		check('undo restored the three deleted views', afterUndo.sheets[0].views.length === 3, { views: afterUndo.sheets[0].views.length, undone });

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
