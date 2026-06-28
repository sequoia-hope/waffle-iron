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

/**
 * If segment a→b is within ~5° of an axis, the orientation locked to that axis
 * ('horizontal' | 'vertical'); otherwise null (slanted → leader chooses). Used
 * to keep a single-line dimension from collapsing onto its degenerate
 * (perpendicular, zero-length) axis.
 */
export function axisAlignedOrientation(a, b) {
	const dx = Math.abs(b.x - a.x);
	const dy = Math.abs(b.y - a.y);
	if (dx === 0 && dy === 0) return null;
	const deg = Math.atan2(dy, dx) * RAD2DEG; // 0 = horizontal, 90 = vertical
	if (deg <= 5) return 'horizontal';
	if (deg >= 85) return 'vertical';
	return null;
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

	// Single line → linear dimension on its endpoints. Both endpoints lie on the
	// SAME line, so the perpendicular axis is always degenerate (zero) — an
	// axis-aligned line must be measured along its own axis (a horizontal line
	// dimensions horizontally, a vertical line vertically). Only a slanted line
	// is leader-driven (where horizontal / vertical / aligned are all non-zero).
	if (targets.length === 1) {
		const line = ent(targets[0].id);
		const ep = line && lineEndpoints(line, positions);
		if (!ep) return null;
		const [a, b] = ep;
		const orientation = axisAlignedOrientation(a, b) ?? orientationFromLeader(a, b, leader);
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

	// line + line → leader-driven linear distance if parallel, else angle.
	if (lines.length === 2) {
		const e0 = lineEndpoints(lines[0], positions);
		const e1 = lineEndpoints(lines[1], positions);
		if (!e0 || !e1) return null;
		if (linesAreParallel(e0[0], e0[1], e1[0], e1[1])) {
			// Treat each line's start point as the representative anchor and let
			// the leader choose horizontal / vertical / aligned, mirroring the
			// point-pair case. Horizontal/vertical measure the axis-aligned gap
			// between the anchors; aligned measures the true perpendicular gap
			// between the (parallel) lines. See /specs/dimension_tool.md.
			const a = e0[0];
			const b = e1[0];
			const orientation = orientationFromLeader(a, b, leader);
			if (orientation === 'horizontal') {
				const value = round4(Math.abs(b.x - a.x));
				return {
					dimKind: 'linear', orientation, value, valueField: 'value',
					constraint: { type: 'HDistance', point_a: lines[0].start_id, point_b: lines[1].start_id, value },
				};
			}
			if (orientation === 'vertical') {
				const value = round4(Math.abs(b.y - a.y));
				return {
					dimKind: 'linear', orientation, value, valueField: 'value',
					constraint: { type: 'VDistance', point_a: lines[0].start_id, point_b: lines[1].start_id, value },
				};
			}
			// aligned → perpendicular distance between the two parallel lines.
			const value = round4(pointLineDistance(b, e0[0], e0[1]));
			return {
				dimKind: 'lineDistance', orientation: 'aligned', value, valueField: 'value',
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

/** Arrowhead length in screen pixels (converted to sketch units via pixelSize). */
const ARROW_PX = 11;

/**
 * Arrowhead at tip `P` pointing along unit dir (ux,uy), as a sub-polyline that
 * both starts and ends at `P` (out-and-back barbs) so it splices cleanly into a
 * single connected stroke without stray connecting segments.
 */
function arrowAt(P, ux, uy, size) {
	const bx = -ux * size; // backward along the dim line
	const by = -uy * size;
	const px = -uy * size * 0.42; // perpendicular half-width
	const py = ux * size * 0.42;
	const a1 = [P[0] + bx + px, P[1] + by + py];
	const a2 = [P[0] + bx - px, P[1] + by - py];
	return [P, a1, P, a2, P];
}

/** Foot of the perpendicular from point p onto the infinite line l1→l2. */
function projectOntoLine(p, l1, l2) {
	const lx = l2.x - l1.x;
	const ly = l2.y - l1.y;
	const ll = lx * lx + ly * ly;
	if (ll < 1e-18) return [l1.x, l1.y];
	const t = ((p.x - l1.x) * lx + (p.y - l1.y) * ly) / ll;
	return [l1.x + lx * t, l1.y + ly * t];
}

/** Unit vector from D2 to D1; falls back to +x for a degenerate segment. */
function unitFrom(d1, d2) {
	const dx = d1[0] - d2[0];
	const dy = d1[1] - d2[1];
	const m = Math.hypot(dx, dy);
	return m < 1e-12 ? [1, 0] : [dx / m, dy / m];
}

/**
 * A linear dimension between dim-line endpoints d1,d2 with optional witness
 * lines back to the measured anchors a,b. Traced as one connected polyline:
 * witnessA → arrow(d1) → dimLine → arrow(d2) → witnessB, with arrowheads
 * pointing outward at each end toward the items being measured.
 */
function dimWithWitness(d1, d2, a, b, size) {
	const [o1x, o1y] = unitFrom(d1, d2); // outward at d1
	const [o2x, o2y] = unitFrom(d2, d1); // outward at d2
	const poly = [];
	if (a) poly.push([a.x, a.y]); // witness A
	poly.push(d1, ...arrowAt(d1, o1x, o1y, size).slice(1));
	poly.push(d2, ...arrowAt(d2, o2x, o2y, size).slice(1));
	if (b) poly.push([b.x, b.y]); // witness B
	return poly;
}

/**
 * Build the full dimension preview polyline (sketch coords) for a classified
 * dimension — witness lines, the dimension line, and outward arrowheads — so the
 * hover clearly hints how a click will land. Returns an array of [x,y], or null.
 *
 * `pixelSize` is sketch-units-per-screen-pixel, used to keep arrowheads a stable
 * on-screen size regardless of zoom.
 */
export function dimensionPreviewPolyline(res, { positions, entities, leader, pixelSize = 0.001 }) {
	if (!res) return null;
	const size = ARROW_PX * pixelSize;
	const ent = (id) => entities.find((e) => e.id === id);

	if (res.dimKind === 'linear') {
		const c = res.constraint;
		const idA = c.point_a ?? c.entity_a;
		const idB = c.point_b ?? c.entity_b;
		const a = positions.get(idA);
		const b = positions.get(idB);
		if (!a || !b) return null;
		let d1;
		let d2;
		if (res.orientation === 'horizontal') {
			d1 = [a.x, leader.y];
			d2 = [b.x, leader.y];
		} else if (res.orientation === 'vertical') {
			d1 = [leader.x, a.y];
			d2 = [leader.x, b.y];
		} else {
			// aligned: dim line parallel to AB, offset through the leader.
			const dx = b.x - a.x;
			const dy = b.y - a.y;
			const len = Math.hypot(dx, dy) || 1;
			const nx = -dy / len;
			const ny = dx / len;
			const off = (leader.x - a.x) * nx + (leader.y - a.y) * ny;
			d1 = [a.x + nx * off, a.y + ny * off];
			d2 = [b.x + nx * off, b.y + ny * off];
		}
		return dimWithWitness(d1, d2, a, b, size);
	}

	if (res.dimKind === 'perp') {
		// point + line: perpendicular from the point to its foot on the line.
		const p = positions.get(res.constraint.point);
		const line = ent(res.constraint.entity);
		const l1 = line && positions.get(line.start_id);
		const l2 = line && positions.get(line.end_id);
		if (!p || !l1 || !l2) return null;
		const foot = projectOntoLine(p, l1, l2);
		return dimWithWitness([p.x, p.y], foot, null, null, size);
	}

	if (res.dimKind === 'lineDistance') {
		// two parallel lines: perpendicular gap at the leader's location, drawn
		// between feet on each line with arrows pointing at each line. The
		// constraint stores line0 as `entity` and line1 via its start-point id.
		const l0 = ent(res.constraint.entity);
		const a0 = l0 && positions.get(l0.start_id);
		const b0 = l0 && positions.get(l0.end_id);
		const startPt = positions.get(res.constraint.point);
		if (!a0 || !b0 || !startPt) return null;
		// Foot of the leader on line0, and on the parallel line through line1's
		// start point (same direction as line0).
		const f0 = projectOntoLine(leader, a0, b0);
		const other = { x: startPt.x + (b0.x - a0.x), y: startPt.y + (b0.y - a0.y) };
		const f1 = projectOntoLine(leader, startPt, other);
		return dimWithWitness(f0, f1, null, null, size);
	}

	if (res.dimKind === 'angle') {
		const line1 = ent(res.constraint.line_a);
		const a = line1 && positions.get(line1.start_id);
		const b = line1 && positions.get(line1.end_id);
		if (!a || !b) return null;
		const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
		return [[mid.x, mid.y], [leader.x, leader.y]];
	}

	return null;
}

/**
 * Back-compat thin wrapper kept for the linear case (witness + dim line, no
 * arrows). Prefer {@link dimensionPreviewPolyline}.
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
	const dx = b.x - a.x;
	const dy = b.y - a.y;
	const len = Math.hypot(dx, dy) || 1;
	const nx = -dy / len;
	const ny = dx / len;
	const off = (leader.x - a.x) * nx + (leader.y - a.y) * ny;
	const a2 = [a.x + nx * off, a.y + ny * off];
	const b2 = [b.x + nx * off, b.y + ny * off];
	return [[a.x, a.y], a2, b2, [b.x, b.y]];
}
