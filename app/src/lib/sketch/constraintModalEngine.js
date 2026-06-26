/**
 * Constraint-modal decision engine (pure).
 *
 * The modal is constraint-FIRST: the active constraint type is fixed and the
 * user keeps picking geometry. This module decides, for each pick, whether to
 * apply the constraint, keep collecting, or reject the pick — and emits the
 * concrete SketchConstraint(s) by reusing the existing builders in
 * constraintLogic.js (NO new constraint math lives here).
 *
 * See /specs/constraint_modal.md for the branch table and invariants.
 */
import { getApplicableConstraints } from './constraintLogic.js';

/**
 * @typedef {'unary'|'chain'|'rolePair'} ConstraintMode
 * @typedef {(kind: string) => boolean} KindPredicate
 */

/**
 * Modal-supported constraints. `keys` lists the getApplicableConstraints()
 * builder keys that realize this constraint, in priority order (first non-null
 * wins for a given entity subset).
 *
 * @type {Record<string, {
 *   label: string,
 *   mode: ConstraintMode,
 *   keys: string[],
 *   accepts?: KindPredicate,
 *   roles?: [KindPredicate, KindPredicate],
 * }>}
 */
export const CONSTRAINT_MODAL_SPECS = {
	horizontal: { label: 'Horizontal', mode: 'unary', keys: ['horizontal'], accepts: (k) => k === 'Line' },
	vertical: { label: 'Vertical', mode: 'unary', keys: ['vertical'], accepts: (k) => k === 'Line' },
	fix: { label: 'Fix', mode: 'unary', keys: ['fix'], accepts: (k) => k === 'Point' },

	coincident: { label: 'Coincident', mode: 'chain', keys: ['coincident'], accepts: (k) => k === 'Point' },
	parallel: { label: 'Parallel', mode: 'chain', keys: ['parallel'], accepts: (k) => k === 'Line' },
	perpendicular: { label: 'Perpendicular', mode: 'chain', keys: ['perpendicular'], accepts: (k) => k === 'Line' },
	equal: { label: 'Equal', mode: 'chain', keys: ['equal', 'equalRadius'], accepts: (k) => k === 'Line' || k === 'Circle' || k === 'Arc' },
	symmetricH: { label: 'Symmetric H', mode: 'chain', keys: ['symmetricH'], accepts: (k) => k === 'Point' },
	symmetricV: { label: 'Symmetric V', mode: 'chain', keys: ['symmetricV'], accepts: (k) => k === 'Point' },

	tangent: { label: 'Tangent', mode: 'rolePair', keys: ['tangent'], roles: [(k) => k === 'Line', (k) => k === 'Circle' || k === 'Arc'] },
	midpoint: { label: 'Midpoint', mode: 'rolePair', keys: ['midpoint'], roles: [(k) => k === 'Point', (k) => k === 'Line'] },
	pointOnLine: { label: 'Point on Entity', mode: 'rolePair', keys: ['pointOnLine', 'pointOnCircle'], roles: [(k) => k === 'Point', (k) => k === 'Line' || k === 'Circle' || k === 'Arc'] },
};

/** Human-readable instruction shown while the modal waits for picks. */
export function modalInstruction(constraintId) {
	const spec = CONSTRAINT_MODAL_SPECS[constraintId];
	if (!spec) return '';
	switch (spec.mode) {
		case 'unary':
			return `Click geometry to make it ${spec.label.toLowerCase()}`;
		case 'chain':
			return `Click geometry to chain ${spec.label.toLowerCase()}`;
		case 'rolePair':
			return `Click the two entities to constrain (${spec.label.toLowerCase()})`;
		default:
			return '';
	}
}

export function isModalConstraint(constraintId) {
	return Object.prototype.hasOwnProperty.call(CONSTRAINT_MODAL_SPECS, constraintId);
}

const entityKind = (e) => (e ? e.type : null);

/**
 * Build the concrete SketchConstraint for an entity subset by reusing the
 * shared builders. Returns null if no candidate builder applies to the subset
 * (e.g. Equal of a line and a circle).
 * @returns {object | null}
 */
function buildConstraint(spec, subsetIds, entities, positions) {
	const applicable = getApplicableConstraints(new Set(subsetIds), entities, positions);
	for (const key of spec.keys) {
		const builder = applicable[key];
		if (builder) return builder();
	}
	return null;
}

const reject = (running, message) => ({ action: 'reject', constraints: [], nextRunning: running, message });
const collect = (running, message) => ({ action: 'collect', constraints: [], nextRunning: running, message: message ?? null });

/**
 * Decide what a single pick does in the active constraint modal.
 *
 * @param {object} args
 * @param {string} args.constraintId
 * @param {number[]} args.running - running entity-id list (meaning depends on mode)
 * @param {number | null} args.pickId - clicked entity id (null = empty space)
 * @param {Array<object>} args.entities
 * @param {Map<number, {x:number,y:number}>} args.positions
 * @returns {{ action: 'apply'|'collect'|'reject', constraints: object[], nextRunning: number[], message: string|null }}
 */
export function stepConstraintModal({ constraintId, running, pickId, entities, positions }) {
	const spec = CONSTRAINT_MODAL_SPECS[constraintId];
	if (!spec) return reject(running, 'Unknown constraint');

	// Empty-space click: inert, keep waiting.
	if (pickId == null) return collect(running, null);

	const pick = entities.find((e) => e.id === pickId);
	const kind = entityKind(pick);
	if (!pick) return reject(running, 'Nothing pickable there');

	if (spec.mode === 'unary') {
		if (!spec.accepts(kind)) return reject(running, `${spec.label} needs a ${describeAccepts(spec)}`);
		const c = buildConstraint(spec, [pickId], entities, positions);
		if (!c) return reject(running, `Can't apply ${spec.label} here`);
		// Each pick is independent; running stays empty.
		return { action: 'apply', constraints: [c], nextRunning: [], message: null };
	}

	if (spec.mode === 'chain') {
		if (!spec.accepts(kind)) return reject(running, `${spec.label} needs a ${describeAccepts(spec)}`);
		const anchor = running.length > 0 ? running[running.length - 1] : null;
		if (anchor == null) return collect([pickId], 'Pick another to apply');
		if (anchor === pickId) return reject(running, 'Pick a different entity');
		const c = buildConstraint(spec, [anchor, pickId], entities, positions);
		if (!c) return reject(running, `Can't ${spec.label.toLowerCase()} those two`);
		// Advance the anchor so the next pick chains off this one.
		return { action: 'apply', constraints: [c], nextRunning: [pickId], message: null };
	}

	// rolePair: collect one entity per distinct role, then apply + reset.
	if (spec.mode === 'rolePair') {
		const [roleA, roleB] = spec.roles;
		if (!roleA(kind) && !roleB(kind)) return reject(running, `${spec.label} can't use a ${kind}`);
		if (running.includes(pickId)) return reject(running, 'Already picked');

		const next = [...running, pickId];
		// Need exactly one entity satisfying each role.
		const fills = assignRoles(next, entities, spec.roles);
		if (!fills) {
			// Two of the same role and no slot for it — reject the duplicate.
			return reject(running, `Need a ${describeRole(spec, running, entities)}`);
		}
		if (fills.complete) {
			const c = buildConstraint(spec, [fills.a, fills.b], entities, positions);
			if (!c) return reject(running, `Can't apply ${spec.label} to those`);
			return { action: 'apply', constraints: [c], nextRunning: [], message: null };
		}
		return collect(next, 'Pick the other entity');
	}

	return reject(running, 'Unsupported');
}

/**
 * Assign the collected entities to the two (non-overlapping) roles by greedy
 * fit. Returns { complete, a, b } — complete when both roles are filled — or
 * null when an entity fits no free role (e.g. a second entity for an
 * already-filled single role).
 */
function assignRoles(ids, entities, roles) {
	const [roleA, roleB] = roles;
	let a = null;
	let b = null;
	for (const id of ids) {
		const k = entityKind(entities.find((x) => x.id === id));
		if (a == null && roleA(k)) a = id;
		else if (b == null && roleB(k)) b = id;
		else return null;
	}
	return { complete: a != null && b != null, a, b };
}

function describeAccepts(spec) {
	if (spec.accepts) {
		if (spec.accepts('Point')) return 'point';
		if (spec.accepts('Line')) return 'line';
	}
	return 'compatible entity';
}

function describeRole(spec, running, entities) {
	// Which role is still unfilled given current running picks.
	const fills = assignRoles(running, entities, spec.roles);
	if (fills && fills.a == null) return 'first entity';
	return 'other entity';
}
