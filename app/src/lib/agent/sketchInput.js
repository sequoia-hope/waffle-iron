/**
 * Shape checks for `sketch_create` input (specs/waffle_mcp_server.md A13).
 *
 * Ids only: unique entity ids, entity references that name existing Points,
 * and constraint references that name existing entities. No geometry is
 * checked here (§6.4): degenerate values go to the solver unchanged. Pure.
 */

/** Entity fields that must name a Point entity. */
const ENTITY_POINT_FIELDS = {
	Line: ['start_id', 'end_id'],
	Circle: ['center_id'],
	Arc: ['center_id', 'start_id', 'end_id']
};

/** Constraint fields that name a sketch entity (see `waffle_types::SketchConstraint`). */
const CONSTRAINT_REF_FIELDS = [
	'point',
	'point_a',
	'point_b',
	'entity',
	'entity_a',
	'entity_b',
	'line',
	'line_a',
	'line_b',
	'line_c',
	'line_d',
	'curve',
	'symmetry_line'
];

/**
 * @param {Array<any>} entities
 * @param {Array<any>} constraints
 * @returns {string | null} the first problem, prefixed with its JSON pointer; null when well formed
 */
export function sketchInputProblem(entities, constraints) {
	/** @type {Map<number, string>} id -> entity type */
	const types = new Map();
	for (let i = 0; i < entities.length; i++) {
		const e = entities[i];
		if (types.has(e.id)) return `/entities/${i}/id: duplicate entity id ${e.id}`;
		types.set(e.id, e.type);
	}

	const isPoint = (id) => types.get(id) === 'Point';
	for (let i = 0; i < entities.length; i++) {
		const e = entities[i];
		for (const field of ENTITY_POINT_FIELDS[e.type] ?? []) {
			if (!isPoint(e[field])) return `/entities/${i}/${field}: no Point entity with id ${e[field]}`;
		}
		if (e.type === 'Spline') {
			for (let k = 0; k < (e.point_ids ?? []).length; k++) {
				if (!isPoint(e.point_ids[k])) {
					return `/entities/${i}/point_ids/${k}: no Point entity with id ${e.point_ids[k]}`;
				}
			}
		}
	}

	for (let i = 0; i < constraints.length; i++) {
		const c = constraints[i];
		for (const field of CONSTRAINT_REF_FIELDS) {
			if (field in c && !types.has(c[field])) {
				return `/constraints/${i}/${field}: no entity with id ${c[field]}`;
			}
		}
	}
	return null;
}
