/**
 * libslvs WASM Constraint Solver Bridge
 *
 * Wraps the Emscripten-compiled SolveSpace constraint solver and provides
 * a high-level API for solving 2D sketch constraints in the Web Worker.
 *
 * Struct layouts (wasm32 ABI):
 *   Slvs_Param:      16 bytes  { h:u32(+0), group:u32(+4), val:f64(+8) }
 *   Slvs_Entity:     56 bytes  { h:u32(+0), group:u32(+4), type:i32(+8), wrkpl:u32(+12),
 *                                point[4]:u32(+16..+28), normal:u32(+32), distance:u32(+36),
 *                                param[4]:u32(+40..+52) }
 *   Slvs_Constraint: 56 bytes  { h:u32(+0), group:u32(+4), type:i32(+8), wrkpl:u32(+12),
 *                                valA:f64(+16), ptA:u32(+24), ptB:u32(+28),
 *                                entityA:u32(+32), entityB:u32(+36), entityC:u32(+40),
 *                                entityD:u32(+44), other:i32(+48), other2:i32(+52) }
 *   Slvs_System:     60 bytes  { param:ptr(+0), params:i32(+4), entity:ptr(+8),
 *                                entities:i32(+12), constraint:ptr(+16), constraints:i32(+20),
 *                                dragged[4]:u32(+24..+36), calculateFaileds:i32(+40),
 *                                failed:ptr(+44), faileds:i32(+48), dof:i32(+52),
 *                                result:i32(+56) }
 */

// Struct sizes
const SZ_PARAM = 16;
const SZ_ENTITY = 56;
const SZ_CONSTRAINT = 56;
const SZ_SYSTEM = 60;

// Entity types
const E_POINT_IN_3D = 50000;
const E_POINT_IN_2D = 50001;
const E_NORMAL_IN_3D = 60000;
const E_DISTANCE = 70000;
const E_WORKPLANE = 80000;
const E_LINE_SEGMENT = 80001;
const E_CIRCLE = 80003;
const E_ARC_OF_CIRCLE = 80004;

// Constraint types
const C = {
	Coincident: 100000,
	PtPtDistance: 100001,
	PtLineDistance: 100003,
	PtOnLine: 100006,
	EqualLength: 100008,
	LengthRatio: 100009,
	Symmetric: 100014,
	SymmetricHoriz: 100015,
	SymmetricVert: 100016,
	AtMidpoint: 100018,
	Horizontal: 100019,
	Vertical: 100020,
	Diameter: 100021,
	PtOnCircle: 100022,
	Angle: 100024,
	Parallel: 100025,
	Perpendicular: 100026,
	ArcLineTangent: 100027,
	EqualRadius: 100029,
	WhereDragged: 100031,
	CurveCurveTangent: 100032
};

// Groups
const G_WP = 1;
const G_SK = 2;

// Result codes
const STATUS = ['okay', 'inconsistent', 'didnt_converge', 'too_many_unknowns'];

let M = null;

/**
 * Initialize the libslvs WASM module.
 * @param {Function} createSlvsModule - Emscripten factory function
 */
export async function initSlvs(createSlvsModule) {
	M = await createSlvsModule({
		locateFile: (path) => `/pkg/slvs/${path}`
	});
}

/** @returns {boolean} */
export function isSlvsReady() {
	return M !== null;
}

/**
 * Solve sketch constraints.
 *
 * @param {Array} entities - Sketch entities [{type, id, ...}]
 * @param {Array} constraints - Sketch constraints [{type, ...}]
 * @param {Object} positions - Point positions as plain object {id: {x, y}}
 * @returns {{ positions: Object, status: string, dof: number, failed: number[] }}
 */
export function solveSketch(entities, constraints, positions) {
	if (!M) {
		return { positions, status: 'not_ready', dof: -1, failed: [] };
	}

	const params = [];
	const ents = [];
	const cons = [];
	let np = 10; // next param handle (1-7 used by workplane)
	let ne = 10; // next entity handle (1-3 used by workplane)
	let nc = 1;

	// Handle resolution maps
	const eMap = new Map(); // sketch entity ID -> slvs entity handle
	const pMap = new Map(); // slvs param handle -> { id, axis }

	// === Workplane (Group 1) ===
	// 3D origin at (0,0,0)
	params.push(mkP(1, G_WP, 0), mkP(2, G_WP, 0), mkP(3, G_WP, 0));
	// Quaternion for XY plane (identity)
	params.push(mkP(4, G_WP, 1), mkP(5, G_WP, 0), mkP(6, G_WP, 0), mkP(7, G_WP, 0));

	ents.push(mkE(1, G_WP, E_POINT_IN_3D, 0, [0, 0, 0, 0], 0, 0, [1, 2, 3, 0]));
	ents.push(mkE(2, G_WP, E_NORMAL_IN_3D, 0, [0, 0, 0, 0], 0, 0, [4, 5, 6, 7]));
	ents.push(mkE(3, G_WP, E_WORKPLANE, 0, [1, 0, 0, 0], 2, 0, [0, 0, 0, 0]));

	const WP = 3;
	const NRM = 2;

	// === Map sketch points ===
	for (const e of entities) {
		if (e.type !== 'Point') continue;
		const pos = positions[e.id] || { x: e.x || 0, y: e.y || 0 };
		const pu = np++,
			pv = np++;
		const eh = ne++;

		params.push(mkP(pu, G_SK, pos.x));
		params.push(mkP(pv, G_SK, pos.y));
		ents.push(mkE(eh, G_SK, E_POINT_IN_2D, WP, [0, 0, 0, 0], 0, 0, [pu, pv, 0, 0]));

		eMap.set(e.id, eh);
		pMap.set(pu, { id: e.id, axis: 'x' });
		pMap.set(pv, { id: e.id, axis: 'y' });
	}

	// === Map lines, circles, arcs ===
	for (const e of entities) {
		if (e.type === 'Point') continue;

		if (e.type === 'Line') {
			const s = eMap.get(e.start_id),
				end = eMap.get(e.end_id);
			if (s == null || end == null) continue;
			const eh = ne++;
			ents.push(mkE(eh, G_SK, E_LINE_SEGMENT, WP, [s, end, 0, 0], 0, 0, [0, 0, 0, 0]));
			eMap.set(e.id, eh);
		} else if (e.type === 'Circle') {
			const c = eMap.get(e.center_id);
			if (c == null) continue;
			const rp = np++;
			params.push(mkP(rp, G_SK, e.radius || 10));
			pMap.set(rp, { id: e.id, axis: 'radius' });
			const de = ne++;
			ents.push(mkE(de, G_SK, E_DISTANCE, WP, [0, 0, 0, 0], 0, 0, [rp, 0, 0, 0]));
			const eh = ne++;
			ents.push(mkE(eh, G_SK, E_CIRCLE, WP, [c, 0, 0, 0], NRM, de, [0, 0, 0, 0]));
			eMap.set(e.id, eh);
		} else if (e.type === 'Arc') {
			const c = eMap.get(e.center_id),
				s = eMap.get(e.start_id),
				end = eMap.get(e.end_id);
			if (c == null || s == null || end == null) continue;
			const eh = ne++;
			ents.push(
				mkE(eh, G_SK, E_ARC_OF_CIRCLE, WP, [c, s, end, 0], NRM, 0, [0, 0, 0, 0])
			);
			eMap.set(e.id, eh);
		}
	}

	// === Map constraints ===
	for (const c of constraints) {
		const mapped = mapConstraint(c, nc++, WP, eMap);
		if (mapped) cons.push(mapped);
	}

	// No sketch params to solve
	if (params.length <= 7) {
		return { positions, status: 'okay', dof: 0, failed: [] };
	}

	// === Solve ===
	const result = callSolver(params, ents, cons);

	// === Read solved positions ===
	const solved = { ...positions };
	for (const sp of result.params) {
		const m = pMap.get(sp.h);
		if (!m) continue;
		const cur = solved[m.id] || { x: 0, y: 0 };
		if (m.axis === 'x') solved[m.id] = { ...cur, x: sp.v };
		else if (m.axis === 'y') solved[m.id] = { ...cur, y: sp.v };
	}

	return {
		positions: solved,
		status: STATUS[result.code] || 'unknown',
		dof: result.dof,
		failed: result.failed
	};
}

// --- Helpers ---

function mkP(h, g, v) {
	return { h, g, v };
}
function mkE(h, g, t, w, pt, n, d, p) {
	return { h, g, t, w, pt, n, d, p };
}

function mapConstraint(c, ch, wp, eMap) {
	const pt = (id) => eMap.get(id) || 0;
	const en = (id) => eMap.get(id) || 0;
	let type,
		valA = 0,
		ptA = 0,
		ptB = 0,
		entityA = 0,
		entityB = 0,
		other = 0,
		other2 = 0;

	switch (c.type) {
		case 'Coincident':
			type = C.Coincident;
			ptA = pt(c.point_a);
			ptB = pt(c.point_b);
			if (!ptA || !ptB) return null;
			break;
		case 'Distance':
			type = C.PtPtDistance;
			valA = c.value || 0;
			ptA = pt(c.point_a);
			ptB = pt(c.point_b);
			if (!ptA || !ptB) return null;
			break;
		case 'Horizontal':
			type = C.Horizontal;
			entityA = en(c.entity);
			if (!entityA) return null;
			break;
		case 'Vertical':
			type = C.Vertical;
			entityA = en(c.entity);
			if (!entityA) return null;
			break;
		case 'Parallel':
			type = C.Parallel;
			entityA = en(c.entity_a);
			entityB = en(c.entity_b);
			if (!entityA || !entityB) return null;
			break;
		case 'Perpendicular':
			type = C.Perpendicular;
			entityA = en(c.entity_a);
			entityB = en(c.entity_b);
			if (!entityA || !entityB) return null;
			break;
		case 'EqualLength':
			type = C.EqualLength;
			entityA = en(c.entity_a);
			entityB = en(c.entity_b);
			if (!entityA || !entityB) return null;
			break;
		case 'Tangent':
			type = C.ArcLineTangent;
			entityA = en(c.entity_a);
			entityB = en(c.entity_b);
			other = c.other || 0;
			if (!entityA || !entityB) return null;
			break;
		case 'Midpoint':
			type = C.AtMidpoint;
			ptA = pt(c.point);
			entityA = en(c.entity);
			if (!ptA || !entityA) return null;
			break;
		case 'PointOnLine':
			type = C.PtOnLine;
			ptA = pt(c.point);
			entityA = en(c.entity);
			if (!ptA || !entityA) return null;
			break;
		case 'PointOnCircle':
			type = C.PtOnCircle;
			ptA = pt(c.point);
			entityA = en(c.entity);
			if (!ptA || !entityA) return null;
			break;
		case 'Symmetric':
			type = C.Symmetric;
			ptA = pt(c.point_a);
			ptB = pt(c.point_b);
			entityA = en(c.entity);
			if (!ptA || !ptB || !entityA) return null;
			break;
		case 'SymmetricHoriz':
			type = C.SymmetricHoriz;
			ptA = pt(c.point_a);
			ptB = pt(c.point_b);
			if (!ptA || !ptB) return null;
			break;
		case 'SymmetricVert':
			type = C.SymmetricVert;
			ptA = pt(c.point_a);
			ptB = pt(c.point_b);
			if (!ptA || !ptB) return null;
			break;
		case 'Angle':
			type = C.Angle;
			valA = c.value_degrees || 0;
			entityA = en(c.entity_a);
			entityB = en(c.entity_b);
			other = c.supplementary ? 1 : 0;
			if (!entityA || !entityB) return null;
			break;
		case 'Diameter':
			type = C.Diameter;
			valA = c.value || 0;
			entityA = en(c.entity);
			if (!entityA) return null;
			break;
		case 'EqualRadius':
			type = C.EqualRadius;
			entityA = en(c.entity_a);
			entityB = en(c.entity_b);
			if (!entityA || !entityB) return null;
			break;
		case 'WhereDragged':
			type = C.WhereDragged;
			ptA = pt(c.point);
			if (!ptA) return null;
			break;
		case 'PointLineDistance':
			type = C.PtLineDistance;
			valA = c.value || 0;
			ptA = pt(c.point);
			entityA = en(c.entity);
			if (!ptA || !entityA) return null;
			break;
		case 'LengthRatio':
			type = C.LengthRatio;
			valA = c.value || 1;
			entityA = en(c.entity_a);
			entityB = en(c.entity_b);
			if (!entityA || !entityB) return null;
			break;
		default:
			return null;
	}

	return {
		h: ch,
		g: G_SK,
		type,
		wrkpl: wp,
		valA,
		ptA,
		ptB,
		entityA,
		entityB,
		entityC: 0,
		entityD: 0,
		other,
		other2
	};
}

/**
 * Allocate structs on Emscripten heap, call Slvs_Solve, read results.
 */
function callSolver(params, ents, cons) {
	const malloc = M._malloc;
	const free = M._free;
	const set = M.setValue;
	const get = M.getValue;

	const pPtr = malloc(params.length * SZ_PARAM);
	const ePtr = malloc(ents.length * SZ_ENTITY);
	const cPtr = cons.length > 0 ? malloc(cons.length * SZ_CONSTRAINT) : 0;
	const fPtr = cons.length > 0 ? malloc(cons.length * 4) : 0;
	const sPtr = malloc(SZ_SYSTEM);

	try {
		// Write params
		for (let i = 0; i < params.length; i++) {
			const b = pPtr + i * SZ_PARAM;
			set(b, params[i].h, 'i32');
			set(b + 4, params[i].g, 'i32');
			set(b + 8, params[i].v, 'double');
		}

		// Write entities
		for (let i = 0; i < ents.length; i++) {
			const b = ePtr + i * SZ_ENTITY;
			set(b, ents[i].h, 'i32');
			set(b + 4, ents[i].g, 'i32');
			set(b + 8, ents[i].t, 'i32');
			set(b + 12, ents[i].w, 'i32');
			for (let j = 0; j < 4; j++) set(b + 16 + j * 4, ents[i].pt[j], 'i32');
			set(b + 32, ents[i].n, 'i32');
			set(b + 36, ents[i].d, 'i32');
			for (let j = 0; j < 4; j++) set(b + 40 + j * 4, ents[i].p[j], 'i32');
		}

		// Write constraints
		for (let i = 0; i < cons.length; i++) {
			const b = cPtr + i * SZ_CONSTRAINT;
			set(b, cons[i].h, 'i32');
			set(b + 4, cons[i].g, 'i32');
			set(b + 8, cons[i].type, 'i32');
			set(b + 12, cons[i].wrkpl, 'i32');
			set(b + 16, cons[i].valA, 'double');
			set(b + 24, cons[i].ptA, 'i32');
			set(b + 28, cons[i].ptB, 'i32');
			set(b + 32, cons[i].entityA, 'i32');
			set(b + 36, cons[i].entityB, 'i32');
			set(b + 40, cons[i].entityC, 'i32');
			set(b + 44, cons[i].entityD, 'i32');
			set(b + 48, cons[i].other, 'i32');
			set(b + 52, cons[i].other2, 'i32');
		}

		// Write Slvs_System
		set(sPtr, pPtr, 'i32');
		set(sPtr + 4, params.length, 'i32');
		set(sPtr + 8, ePtr, 'i32');
		set(sPtr + 12, ents.length, 'i32');
		set(sPtr + 16, cPtr, 'i32');
		set(sPtr + 20, cons.length, 'i32');
		for (let i = 0; i < 4; i++) set(sPtr + 24 + i * 4, 0, 'i32'); // dragged
		set(sPtr + 40, 1, 'i32'); // calculateFaileds
		set(sPtr + 44, fPtr, 'i32');
		set(sPtr + 48, cons.length, 'i32'); // faileds capacity
		set(sPtr + 52, 0, 'i32'); // dof (output)
		set(sPtr + 56, 0, 'i32'); // result (output)

		// Solve
		M._Slvs_Solve(sPtr, G_SK);

		// Read results
		const code = get(sPtr + 56, 'i32');
		const dof = get(sPtr + 52, 'i32');
		const nf = get(sPtr + 48, 'i32');

		const solvedParams = [];
		for (let i = 0; i < params.length; i++) {
			const b = pPtr + i * SZ_PARAM;
			solvedParams.push({ h: get(b, 'i32'), v: get(b + 8, 'double') });
		}

		const failed = [];
		for (let i = 0; i < nf; i++) {
			failed.push(get(fPtr + i * 4, 'i32'));
		}

		return { params: solvedParams, code, dof, failed };
	} finally {
		free(pPtr);
		free(ePtr);
		if (cPtr) free(cPtr);
		if (fPtr) free(fPtr);
		free(sPtr);
	}
}
