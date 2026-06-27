/**
 * Shared constraint-applicability logic.
 *
 * Determines which constraints can be applied to the current selection.
 * Used by both ConstraintMenu (right-click) and Toolbar buttons.
 */

/**
 * Given the current selection, entities, and positions, return an object
 * mapping each constraint type to either a builder function or null.
 *
 * @param {Set<number>} selectionIds - Currently selected entity IDs
 * @param {Array<object>} entities - All sketch entities
 * @param {Map<number, {x: number, y: number}>} positions - Solved positions
 * @returns {Record<string, (() => object) | null>}
 */
export function getApplicableConstraints(selectionIds, entities, positions) {
	const sel = [...selectionIds];
	const selected = sel.map(id => entities.find(e => e.id === id)).filter(Boolean);

	const points = selected.filter(e => e.type === 'Point');
	const lines = selected.filter(e => e.type === 'Line');
	const circles = selected.filter(e => e.type === 'Circle');
	const arcs = selected.filter(e => e.type === 'Arc');

	/** @type {Record<string, (() => object) | null>} */
	const result = {
		horizontal: null,
		vertical: null,
		coincident: null,
		perpendicular: null,
		parallel: null,
		equal: null,
		tangent: null,
		midpoint: null,
		fix: null,
		distance: null,
		radius: null,
		angle: null,
		symmetric: null,
		symmetricH: null,
		symmetricV: null,
		pointOnLine: null,
		pointOnCircle: null,
		equalRadius: null,
		lengthRatio: null,
		pointLineDistance: null,
		diameter: null,
		hDistance: null,
		vDistance: null,
	};

	// 1 line only
	if (lines.length === 1 && points.length === 0 && circles.length === 0 && arcs.length === 0) {
		result.horizontal = () => ({ type: 'Horizontal', entity: lines[0].id });
		result.vertical = () => ({ type: 'Vertical', entity: lines[0].id });
		result.distance = () => {
			// Compute actual length for default value
			const p1 = positions.get(lines[0].start_id);
			const p2 = positions.get(lines[0].end_id);
			let len = 1.0;
			if (p1 && p2) {
				const dx = p2.x - p1.x, dy = p2.y - p1.y;
				len = Math.sqrt(dx * dx + dy * dy);
			}
			return { type: 'Distance', entity_a: lines[0].start_id, entity_b: lines[0].end_id, value: len };
		};
	}

	// 2 points
	if (points.length === 2 && lines.length === 0 && circles.length === 0 && arcs.length === 0) {
		result.coincident = () => ({ type: 'Coincident', point_a: points[0].id, point_b: points[1].id });
		result.distance = () => {
			const pA = positions.get(points[0].id);
			const pB = positions.get(points[1].id);
			let len = 1.0;
			if (pA && pB) {
				const dx = pB.x - pA.x, dy = pB.y - pA.y;
				len = Math.sqrt(dx * dx + dy * dy);
			}
			return { type: 'Distance', entity_a: points[0].id, entity_b: points[1].id, value: len };
		};
		result.symmetricH = () => ({ type: 'SymmetricH', point_a: points[0].id, point_b: points[1].id });
		result.symmetricV = () => ({ type: 'SymmetricV', point_a: points[0].id, point_b: points[1].id });
		// Point-pair Horizontal/Vertical: align two points along an axis.
		// (Line form emits { type:'Horizontal', entity } in the 1-line branch.)
		result.horizontal = () => ({ type: 'HorizontalPoints', point_a: points[0].id, point_b: points[1].id });
		result.vertical = () => ({ type: 'VerticalPoints', point_a: points[0].id, point_b: points[1].id });
		result.hDistance = () => {
			const pA = positions.get(points[0].id);
			const pB = positions.get(points[1].id);
			let hDist = 1.0;
			if (pA && pB) hDist = Math.abs(pB.x - pA.x);
			return { type: 'HDistance', point_a: points[0].id, point_b: points[1].id, value: hDist || 1.0 };
		};
		result.vDistance = () => {
			const pA = positions.get(points[0].id);
			const pB = positions.get(points[1].id);
			let vDist = 1.0;
			if (pA && pB) vDist = Math.abs(pB.y - pA.y);
			return { type: 'VDistance', point_a: points[0].id, point_b: points[1].id, value: vDist || 1.0 };
		};
	}

	// 2 lines
	if (lines.length === 2 && points.length === 0 && circles.length === 0 && arcs.length === 0) {
		result.parallel = () => ({ type: 'Parallel', line_a: lines[0].id, line_b: lines[1].id });
		result.perpendicular = () => ({ type: 'Perpendicular', line_a: lines[0].id, line_b: lines[1].id });
		result.equal = () => ({ type: 'Equal', entity_a: lines[0].id, entity_b: lines[1].id });
		result.angle = () => {
			// Compute current angle between the two lines
			const l0s = positions.get(lines[0].start_id);
			const l0e = positions.get(lines[0].end_id);
			const l1s = positions.get(lines[1].start_id);
			const l1e = positions.get(lines[1].end_id);
			let degrees = 45;
			if (l0s && l0e && l1s && l1e) {
				const dx0 = l0e.x - l0s.x, dy0 = l0e.y - l0s.y;
				const dx1 = l1e.x - l1s.x, dy1 = l1e.y - l1s.y;
				const dot = dx0 * dx1 + dy0 * dy1;
				const mag0 = Math.sqrt(dx0 * dx0 + dy0 * dy0);
				const mag1 = Math.sqrt(dx1 * dx1 + dy1 * dy1);
				if (mag0 > 1e-10 && mag1 > 1e-10) {
					degrees = Math.acos(Math.min(1, Math.max(-1, dot / (mag0 * mag1)))) * (180 / Math.PI);
				}
			}
			return { type: 'Angle', line_a: lines[0].id, line_b: lines[1].id, value_degrees: parseFloat(degrees.toFixed(2)) };
		};
		result.lengthRatio = () => ({
			type: 'LengthRatio', entity_a: lines[0].id, entity_b: lines[1].id, value: 1.0
		});
	}

	// 1 point + 1 line
	if (points.length === 1 && lines.length === 1 && circles.length === 0 && arcs.length === 0) {
		result.midpoint = () => ({ type: 'Midpoint', point: points[0].id, line: lines[0].id });
		result.pointOnLine = () => ({ type: 'OnEntity', point: points[0].id, entity: lines[0].id, entityType: 'Line' });
		result.pointLineDistance = () => {
			// Compute actual perpendicular distance
			const pos = positions.get(points[0].id);
			const p1 = positions.get(lines[0].start_id);
			const p2 = positions.get(lines[0].end_id);
			let dist = 1.0;
			if (pos && p1 && p2) {
				const lx = p2.x - p1.x, ly = p2.y - p1.y;
				const lLen = Math.sqrt(lx * lx + ly * ly);
				if (lLen > 1e-10) {
					dist = Math.abs((pos.x - p1.x) * ly - (pos.y - p1.y) * lx) / lLen;
				}
			}
			return { type: 'PointLineDistance', point: points[0].id, entity: lines[0].id, value: parseFloat(dist.toFixed(4)) };
		};
		result.distance = () => ({ type: 'PointLineDistance', point: points[0].id, entity: lines[0].id, value: 1.0 });
	}

	// 1 point + 1 circle
	if (points.length === 1 && circles.length === 1 && lines.length === 0 && arcs.length === 0) {
		result.pointOnCircle = () => ({ type: 'OnEntity', point: points[0].id, entity: circles[0].id, entityType: 'Circle' });
	}

	// 1 point + 1 arc
	if (points.length === 1 && arcs.length === 1 && lines.length === 0 && circles.length === 0) {
		result.pointOnCircle = () => ({ type: 'OnEntity', point: points[0].id, entity: arcs[0].id, entityType: 'Arc' });
	}

	// 1 circle or arc
	if ((circles.length === 1 || arcs.length === 1) && points.length === 0 && lines.length === 0) {
		const entity = circles[0] || arcs[0];
		let radius = entity.radius || 1.0;
		if (entity.type === 'Arc') {
			const center = positions.get(entity.center_id);
			const start = positions.get(entity.start_id);
			if (center && start) {
				const dx = start.x - center.x, dy = start.y - center.y;
				radius = Math.sqrt(dx * dx + dy * dy);
			}
		}
		result.radius = () => ({ type: 'Diameter', entity: entity.id, value: radius * 2 });
		result.diameter = () => ({ type: 'Diameter', entity: entity.id, value: radius * 2 });
	}

	// 2 circles/arcs
	if ((circles.length + arcs.length) === 2 && points.length === 0 && lines.length === 0) {
		const allCircular = [...circles, ...arcs];
		result.equalRadius = () => ({
			type: 'EqualRadius', entity_a: allCircular[0].id, entity_b: allCircular[1].id
		});
		result.equal = () => ({
			type: 'EqualRadius', entity_a: allCircular[0].id, entity_b: allCircular[1].id
		});
	}

	// 1 line + 1 arc
	if (lines.length === 1 && arcs.length === 1 && points.length === 0 && circles.length === 0) {
		result.tangent = () => ({ type: 'Tangent', line: lines[0].id, curve: arcs[0].id });
	}

	// 1 line + 1 circle
	if (lines.length === 1 && circles.length === 1 && points.length === 0 && arcs.length === 0) {
		result.tangent = () => ({ type: 'Tangent', line: lines[0].id, curve: circles[0].id });
	}

	// 2 points + 1 line (symmetric about line)
	if (points.length === 2 && lines.length === 1 && circles.length === 0 && arcs.length === 0) {
		result.symmetric = () => ({
			type: 'Symmetric', entity_a: points[0].id, entity_b: points[1].id, entity_c: lines[0].id
		});
	}

	// 1 point — fix (WhereDragged)
	if (points.length === 1 && lines.length === 0 && circles.length === 0 && arcs.length === 0) {
		const pos = positions.get(points[0].id);
		result.fix = () => ({
			type: 'WhereDragged',
			point: points[0].id,
			x: pos?.x ?? 0,
			y: pos?.y ?? 0
		});
	}

	return result;
}
