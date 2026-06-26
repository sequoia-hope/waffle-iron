/**
 * Dimension tool heuristic (pure).
 *
 * Decides, for a set of picked targets and a leader (cursor) position, what is
 * being dimensioned, in which orientation, the measured value, and which
 * SketchConstraint to emit. Points + lines only — circles/arcs are handled by
 * the tool's immediate radius popup, not here. See /specs/dimension_tool.md.
 */

const RAD2DEG = 180 / Math.PI;

/** A target the tool collected: { id, type } where type is the entity type. */

/** Resolve a line entity to its two solved endpoints, or null. */
function lineEndpoints(line, positions) {
	const a = positions.get(line.start_id);
	const b = positions.get(line.end_id);
	if (!a || !b) return null;
	return [a, b];
}

/**
 * Orientation of a linear dimension from the leader position relative to the
 * two anchors. Returns 'horizontal' | 'vertical' | 'aligned'.
 * See the heuristic in /specs/dimension_tool.md.
 */
export function orientationFromLeader(a, b, leader) {
	const mx = (a.x + b.x) / 2;
	const my = (a.y + b.y) / 2;
	const ox = leader.x - mx;
	const oy = leader.y - my;
	if (Math.hypot(ox, oy) < 1e-9) return 'aligned';
	const deg = Math.atan2(Math.abs(oy), Math.abs(ox)) * RAD2DEG;
	if (deg <= 30) return 'vertical'; // leader to the side → measure Δy
	if (deg >= 60) return 'horizontal'; // leader above/below → measure Δx
	return 'aligned';
}

/** Measured value for a linear orientation between two points. */
function linearValue(orientation, a, b) {
	if (orientation === 'horizontal') return Math.abs(b.x - a.x);
	if (orientation === 'vertical') return Math.abs(b.y - a.y);
	return Math.hypot(b.x - a.x, b.y - a.y);
}

/** Perpendicular distance from point p to the infinite line through l1,l2. */
function pointLineDistance(p, l1, l2) {
	const lx = l2.x - l1.x;
	const ly = l2.y - l1.y;
	const len = Math.hypot(lx, ly);
	if (len < 1e-12) return Math.hypot(p.x - l1.x, p.y - l1.y);
	return Math.abs((p.x - l1.x) * ly - (p.y - l1.y) * lx) / len;
}

/** Interior angle (degrees, 0..180) between directed segments. */
function angleBetweenLines(a1, a2, b1, b2) {
	const ux = a2.x - a1.x;
	const uy = a2.y - a1.y;
	const vx = b2.x - b1.x;
	const vy = b2.y - b1.y;
	const mu = Math.hypot(ux, uy);
	const mv = Math.hypot(vx, vy);
	if (mu < 1e-12 || mv < 1e-12) return 0;
	const c = Math.min(1, Math.max(-1, (ux * vx + uy * vy) / (mu * mv)));
	return Math.acos(c) * RAD2DEG;
}

/** True when the two segments are within ~3° of parallel (or anti-parallel). */
function linesAreParallel(a1, a2, b1, b2) {
	const ang = angleBetweenLines(a1, a2, b1, b2);
	return ang <= 3 || ang >= 177;
}

const round4 = (v) => parseFloat(v.toFixed(4));

/**
 * Whether the picked target set is ready to place a dimension. A single line,
 * or a pair of point/line entities. A lone point waits; circles/arcs are not
 * handled here (the tool dimensions them immediately).
 */
export function isDimensionComplete(targets) {
	if (targets.length === 1) return targets[0].type === 'Line';
	if (targets.length === 2) {
		return targets.every((t) => t.type === 'Point' || t.type === 'Line');
	}
	return false;
}

/**
 * Classify a complete dimension pick.
 *
 * @returns {{
 *   dimKind: 'linear'|'perp'|'lineDistance'|'angle',
 *   orientation?: 'horizontal'|'vertical'|'aligned',
 *   value: number,
 *   valueField: 'value'|'value_degrees',
 *   constraint: object,   // value/value_degrees already set to the measurement
 * } | null}
 */
export function classifyDimension({ targets, leader, positions, entities }) {
	if (!isDimensionComplete(targets)) return null;
	const ent = (id) => entities.find((e) => e.id === id);

	// Single line → treat as a linear dimension on its endpoints.
	if (targets.length === 1) {
		const line = ent(targets[0].id);
		const ep = line && lineEndpoints(line, positions);
		if (!ep) return null;
		const [a, b] = ep;
		const orientation = orientationFromLeader(a, b, leader);
		return linearResult(orientation, a, b, line.start_id, line.end_id);
	}

	const t0 = ent(targets[0].id);
	const t1 = ent(targets[1].id);
	if (!t0 || !t1) return null;

	const points = [t0, t1].filter((e) => e.type === 'Point');
	const lines = [t0, t1].filter((e) => e.type === 'Line');

	// point + point → linear.
	if (points.length === 2) {
		const a = positions.get(points[0].id);
		const b = positions.get(points[1].id);
		if (!a || !b) return null;
		const orientation = orientationFromLeader(a, b, leader);
		return linearResult(orientation, a, b, points[0].id, points[1].id);
	}

	// point + line → perpendicular distance.
	if (points.length === 1 && lines.length === 1) {
		const p = positions.get(points[0].id);
		const ep = lineEndpoints(lines[0], positions);
		if (!p || !ep) return null;
		const value = round4(pointLineDistance(p, ep[0], ep[1]));
		return {
			dimKind: 'perp',
			value,
			valueField: 'value',
			constraint: { type: 'PointLineDistance', point: points[0].id, entity: lines[0].id, value },
		};
	}

	// line + line → distance if parallel, else angle.
	if (lines.length === 2) {
		const e0 = lineEndpoints(lines[0], positions);
		const e1 = lineEndpoints(lines[1], positions);
		if (!e0 || !e1) return null;
		if (linesAreParallel(e0[0], e0[1], e1[0], e1[1])) {
			const value = round4(pointLineDistance(e1[0], e0[0], e0[1]));
			return {
				dimKind: 'lineDistance',
				value,
				valueField: 'value',
				constraint: { type: 'PointLineDistance', point: lines[1].start_id, entity: lines[0].id, value },
			};
		}
		const value = round4(angleBetweenLines(e0[0], e0[1], e1[0], e1[1]));
		return {
			dimKind: 'angle',
			value,
			valueField: 'value_degrees',
			constraint: { type: 'Angle', line_a: lines[0].id, line_b: lines[1].id, value_degrees: value },
		};
	}

	return null;
}

function linearResult(orientation, a, b, idA, idB) {
	const value = round4(linearValue(orientation, a, b));
	let constraint;
	if (orientation === 'horizontal') constraint = { type: 'HDistance', point_a: idA, point_b: idB, value };
	else if (orientation === 'vertical') constraint = { type: 'VDistance', point_a: idA, point_b: idB, value };
	else constraint = { type: 'Distance', entity_a: idA, entity_b: idB, value };
	return { dimKind: 'linear', orientation, value, valueField: 'value', constraint };
}

/**
 * Build the dimension leader/witness preview polyline (sketch coords) for a
 * classified linear dimension, traced as witnessA → dimLine → witnessB so it
 * renders as one polyline. Returns an array of [x,y] points, or null.
 */
export function linearPreviewPolyline(orientation, a, b, leader) {
	if (orientation === 'horizontal') {
		const y = leader.y;
		return [[a.x, a.y], [a.x, y], [b.x, y], [b.x, b.y]];
	}
	if (orientation === 'vertical') {
		const x = leader.x;
		return [[a.x, a.y], [x, a.y], [x, b.y], [b.x, b.y]];
	}
	// aligned: dim line parallel to AB passing through the leader.
	const dx = b.x - a.x;
	const dy = b.y - a.y;
	const len = Math.hypot(dx, dy) || 1;
	// unit perpendicular
	const nx = -dy / len;
	const ny = dx / len;
	// signed offset of leader along the perpendicular from A
	const off = (leader.x - a.x) * nx + (leader.y - a.y) * ny;
	const a2 = [a.x + nx * off, a.y + ny * off];
	const b2 = [b.x + nx * off, b.y + ny * off];
	return [[a.x, a.y], a2, b2, [b.x, b.y]];
}
