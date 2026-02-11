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
	}

	// 2 lines
	if (lines.length === 2 && points.length === 0 && circles.length === 0 && arcs.length === 0) {
		result.parallel = () => ({ type: 'Parallel', line_a: lines[0].id, line_b: lines[1].id });
		result.perpendicular = () => ({ type: 'Perpendicular', line_a: lines[0].id, line_b: lines[1].id });
		result.equal = () => ({ type: 'Equal', entity_a: lines[0].id, entity_b: lines[1].id });
	}

	// 1 point + 1 line
	if (points.length === 1 && lines.length === 1 && circles.length === 0 && arcs.length === 0) {
		result.midpoint = () => ({ type: 'Midpoint', point: points[0].id, line: lines[0].id });
		result.distance = () => ({ type: 'Distance', entity_a: points[0].id, entity_b: lines[0].id, value: 1.0 });
	}

	// 1 circle or arc
	if ((circles.length === 1 || arcs.length === 1) && points.length === 0 && lines.length === 0) {
		const entity = circles[0] || arcs[0];
		result.radius = () => ({ type: 'Radius', entity: entity.id, value: entity.radius || 1.0 });
	}

	// 1 line + 1 arc
	if (lines.length === 1 && arcs.length === 1 && points.length === 0 && circles.length === 0) {
		result.tangent = () => ({ type: 'Tangent', line: lines[0].id, curve: arcs[0].id });
	}

	// 1 line + 1 circle
	if (lines.length === 1 && circles.length === 1 && points.length === 0 && arcs.length === 0) {
		result.tangent = () => ({ type: 'Tangent', line: lines[0].id, curve: circles[0].id });
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
