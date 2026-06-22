/**
 * Auto-snap detection for sketch drawing tools.
 *
 * Detects potential auto-constraints (coincident, horizontal, vertical, on-entity)
 * and returns snapped coordinates + snap indicators for visual feedback.
 */

import { findPointNear, findLineNear, findCircleNear, getSketchPositions, getSketchEntities, getSnapSettings, getReferenceSnapPoints } from '$lib/engine/store.svelte.js';
import { COINCIDENT_SNAP_PX, ON_ENTITY_SNAP_PX, HV_ANGLE_DEG } from '$lib/config.js';

/**
 * @typedef {{ type: 'coincident', x: number, y: number, pointId: number }} CoincidentSnap
 * @typedef {{ type: 'horizontal', x: number, y: number, fromX: number, fromY: number }} HorizontalSnap
 * @typedef {{ type: 'vertical', x: number, y: number, fromX: number, fromY: number }} VerticalSnap
 * @typedef {{ type: 'on-entity', x: number, y: number, entityId: number }} OnEntitySnap
 * @typedef {{ type: 'tangent', x: number, y: number, entityId: number }} TangentSnap
 * @typedef {{ type: 'perpendicular', x: number, y: number, entityId: number }} PerpendicularSnap
 * @typedef {{ type: 'midpoint', x: number, y: number, entityId: number }} MidpointSnap
 * @typedef {{ type: 'quadrant', x: number, y: number, entityId: number }} QuadrantSnap
 * @typedef {{ type: 'origin', x: number, y: number }} OriginSnap
 * @typedef {{ type: 'reference', x: number, y: number, sourceId: string }} ReferenceSnap
 * @typedef {CoincidentSnap | HorizontalSnap | VerticalSnap | OnEntitySnap | TangentSnap | PerpendicularSnap | MidpointSnap | QuadrantSnap | OriginSnap | ReferenceSnap} SnapIndicator
 */

/**
 * @typedef {{ x: number, y: number, snapPointId?: number, constraints: Array<object>, indicator: SnapIndicator | null }} SnapResult
 */

/**
 * Detect snaps for a cursor position.
 *
 * @param {number} x - Cursor sketch X
 * @param {number} y - Cursor sketch Y
 * @param {number | null} fromPointId - The point we're drawing from (for H/V detection)
 * @param {number} screenPixelSize - Sketch units per pixel (for adaptive thresholds)
 * @returns {SnapResult}
 */
export function detectSnaps(x, y, fromPointId, screenPixelSize) {
	const settings = getSnapSettings();
	const coincidentThreshold = (settings.coincidentPx ?? COINCIDENT_SNAP_PX) * screenPixelSize;
	const onEntityThreshold = (settings.onEntityPx ?? ON_ENTITY_SNAP_PX) * screenPixelSize;
	const hvAngleDeg = settings.hvAngleDeg ?? HV_ANGLE_DEG;

	// 1. Coincident snap — highest priority
	const nearPoint = findPointNear(x, y, coincidentThreshold);
	if (nearPoint && nearPoint.id !== fromPointId) {
		return {
			x: nearPoint.x,
			y: nearPoint.y,
			snapPointId: nearPoint.id,
			constraints: [],
			indicator: { type: 'coincident', x: nearPoint.x, y: nearPoint.y, pointId: nearPoint.id }
		};
	}

	// 1b. Origin snap — pin point to (0,0) via WhereDragged
	const originDist = Math.sqrt(x * x + y * y);
	if (originDist < coincidentThreshold) {
		return {
			x: 0, y: 0,
			constraints: [
				{ type: 'WhereDragged', x: 0, y: 0 }
			],
			indicator: { type: 'origin', x: 0, y: 0 }
		};
	}

	// 1b2. Reference point snap — snap to points from inactive sketches
	{
		const refPoints = getReferenceSnapPoints();
		let closestRef = null;
		let closestRefDist = coincidentThreshold;
		for (const rp of refPoints) {
			const dx = rp.x - x;
			const dy = rp.y - y;
			const dist = Math.sqrt(dx * dx + dy * dy);
			if (dist < closestRefDist) {
				closestRefDist = dist;
				closestRef = rp;
			}
		}
		if (closestRef) {
			return {
				x: closestRef.x, y: closestRef.y,
				constraints: [{ type: 'WhereDragged', x: closestRef.x, y: closestRef.y }],
				indicator: { type: 'reference', x: closestRef.x, y: closestRef.y, sourceId: closestRef.sourceId }
			};
		}
	}

	// 1c. Midpoint snap — snap to midpoint of line entities
	{
		const entities = getSketchEntities();
		const positions = getSketchPositions();
		for (const entity of entities) {
			if (entity.type !== 'Line') continue;
			const p1 = positions.get(entity.start_id);
			const p2 = positions.get(entity.end_id);
			if (!p1 || !p2) continue;
			const midX = (p1.x + p2.x) / 2;
			const midY = (p1.y + p2.y) / 2;
			const dist = Math.sqrt((x - midX) ** 2 + (y - midY) ** 2);
			if (dist < coincidentThreshold) {
				return {
					x: midX, y: midY,
					// Pin the new point to the line's midpoint. The point id is not
					// known until the entity is created, so emit a template carrying
					// the line id; applyPointSnapConstraints fills in `point`.
					constraints: [{ type: 'Midpoint', line: entity.id }],
					indicator: { type: 'midpoint', x: midX, y: midY, entityId: entity.id }
				};
			}
		}
	}

	// 1d. Quadrant snap — snap to 0/90/180/270 points on circles and arcs
	{
		const entities = getSketchEntities();
		const positions = getSketchPositions();
		const quadrantAngles = [0, Math.PI / 2, Math.PI, 3 * Math.PI / 2];
		for (const entity of entities) {
			if (entity.type !== 'Circle' && entity.type !== 'Arc') continue;
			const center = positions.get(entity.center_id);
			if (!center) continue;

			let radius;
			if (entity.type === 'Circle') {
				radius = entity.radius;
			} else {
				const startPt = positions.get(entity.start_id);
				if (!startPt) continue;
				radius = Math.sqrt((startPt.x - center.x) ** 2 + (startPt.y - center.y) ** 2);
			}

			for (const angle of quadrantAngles) {
				// For arcs, check if this quadrant angle is within the arc's range
				if (entity.type === 'Arc') {
					const startPt = positions.get(entity.start_id);
					const endPt = positions.get(entity.end_id);
					if (!startPt || !endPt) continue;
					let startAngle = Math.atan2(startPt.y - center.y, startPt.x - center.x);
					let endAngle = Math.atan2(endPt.y - center.y, endPt.x - center.x);
					if (endAngle <= startAngle) endAngle += Math.PI * 2;
					let testAngle = angle;
					while (testAngle < startAngle) testAngle += Math.PI * 2;
					if (testAngle > endAngle) continue;
				}

				const qx = center.x + radius * Math.cos(angle);
				const qy = center.y + radius * Math.sin(angle);
				const dist = Math.sqrt((x - qx) ** 2 + (y - qy) ** 2);
				if (dist < coincidentThreshold) {
					return {
						x: qx, y: qy,
						constraints: [],
						indicator: { type: 'quadrant', x: qx, y: qy, entityId: entity.id }
					};
				}
			}
		}
	}

	// 2. Horizontal / Vertical snap — when drawing from a known point
	if (fromPointId != null) {
		const positions = getSketchPositions();
		const fromPos = positions.get(fromPointId);
		if (fromPos) {
			const dx = x - fromPos.x;
			const dy = y - fromPos.y;
			const len = Math.sqrt(dx * dx + dy * dy);
			if (len > 0.000001) {
				const angleDeg = Math.abs(Math.atan2(dy, dx)) * (180 / Math.PI);
				// Near horizontal (angle near 0 or 180)
				if (angleDeg < hvAngleDeg || angleDeg > (180 - hvAngleDeg)) {
					return {
						x, y: fromPos.y,
						constraints: [{ type: 'Horizontal' }],
						indicator: { type: 'horizontal', x, y: fromPos.y, fromX: fromPos.x, fromY: fromPos.y }
					};
				}
				// Near vertical (angle near 90)
				if (Math.abs(angleDeg - 90) < hvAngleDeg) {
					return {
						x: fromPos.x, y,
						constraints: [{ type: 'Vertical' }],
						indicator: { type: 'vertical', x: fromPos.x, y, fromX: fromPos.x, fromY: fromPos.y }
					};
				}
			}
		}
	}

	// 3. On-entity snap — snap to nearest point on a line or circle
	const nearLine = findLineNear(x, y, onEntityThreshold);
	if (nearLine) {
		const entities = getSketchEntities();
		const line = entities.find(e => e.type === 'Line' && e.id === nearLine.id);
		if (line) {
			const positions = getSketchPositions();
			const p1 = positions.get(line.start_id);
			const p2 = positions.get(line.end_id);
			if (p1 && p2) {
				// Project cursor onto line segment
				const snapped = projectOntoSegment(x, y, p1.x, p1.y, p2.x, p2.y);
				return {
					x: snapped.x, y: snapped.y,
					constraints: [{ type: 'OnEntity', entity: nearLine.id }],
					indicator: { type: 'on-entity', x: snapped.x, y: snapped.y, entityId: nearLine.id }
				};
			}
		}
	}

	const nearCircle = findCircleNear(x, y, onEntityThreshold);
	if (nearCircle) {
		const entities = getSketchEntities();
		const circle = entities.find(e => e.type === 'Circle' && e.id === nearCircle.id);
		if (circle) {
			const positions = getSketchPositions();
			const center = positions.get(circle.center_id);
			if (center) {
				// Snap to nearest point on circumference
				const dx = x - center.x;
				const dy = y - center.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				if (dist > 0.000001) {
					const sx = center.x + (dx / dist) * circle.radius;
					const sy = center.y + (dy / dist) * circle.radius;
					return {
						x: sx, y: sy,
						constraints: [{ type: 'OnEntity', entity: nearCircle.id }],
						indicator: { type: 'on-entity', x: sx, y: sy, entityId: nearCircle.id }
					};
				}
			}
		}
	}

	// 4. Tangent snap — when drawing from a point, check if the cursor forms a tangent to a circle/arc
	if (fromPointId != null) {
		const tangent = detectTangentSnap(x, y, fromPointId, onEntityThreshold);
		if (tangent) return tangent;
	}

	// 5. Perpendicular snap — when drawing from a point, check if cursor forms perpendicular to a line
	if (fromPointId != null) {
		const perp = detectPerpendicularSnap(x, y, fromPointId, onEntityThreshold);
		if (perp) return perp;
	}

	// No snap
	return { x, y, constraints: [], indicator: null };
}

/**
 * Project a point onto a line segment, clamped to [0, 1].
 */
function projectOntoSegment(px, py, ax, ay, bx, by) {
	const abx = bx - ax;
	const aby = by - ay;
	const len2 = abx * abx + aby * aby;
	if (len2 < 1e-12) return { x: ax, y: ay };
	let t = ((px - ax) * abx + (py - ay) * aby) / len2;
	t = Math.max(0, Math.min(1, t));
	return { x: ax + t * abx, y: ay + t * aby };
}

/**
 * Detect tangent snap from a point to a circle/arc.
 * A line from fromPoint to the tangent point on a circle is perpendicular to the radius.
 *
 * @param {number} x - Cursor X
 * @param {number} y - Cursor Y
 * @param {number} fromPointId - Point we're drawing from
 * @param {number} threshold - Snap threshold in sketch units
 * @returns {SnapResult | null}
 */
function detectTangentSnap(x, y, fromPointId, threshold) {
	const positions = getSketchPositions();
	const fromPos = positions.get(fromPointId);
	if (!fromPos) return null;

	const entities = getSketchEntities();

	for (const entity of entities) {
		if (entity.type !== 'Circle' && entity.type !== 'Arc') continue;
		const center = positions.get(entity.center_id);
		if (!center) continue;

		let radius;
		if (entity.type === 'Circle') {
			radius = entity.radius;
		} else {
			const startPt = positions.get(entity.start_id);
			if (!startPt) continue;
			radius = Math.sqrt((startPt.x - center.x) ** 2 + (startPt.y - center.y) ** 2);
		}

		// Distance from fromPoint to center
		const dx = fromPos.x - center.x;
		const dy = fromPos.y - center.y;
		const distToCenter = Math.sqrt(dx * dx + dy * dy);

		// Tangent only makes sense if fromPoint is outside the circle
		if (distToCenter <= radius) continue;

		// Tangent length from fromPoint to tangent point: sqrt(d^2 - r^2)
		const tangentLen = Math.sqrt(distToCenter * distToCenter - radius * radius);

		// Angle from center to fromPoint
		const baseAngle = Math.atan2(dy, dx);
		// Half-angle of the tangent: asin(r/d)
		const halfAngle = Math.asin(radius / distToCenter);

		// Two tangent points
		const tangentPoints = [
			{
				x: center.x + radius * Math.cos(baseAngle + Math.PI / 2 + halfAngle - Math.PI / 2),
				y: center.y + radius * Math.sin(baseAngle + Math.PI / 2 + halfAngle - Math.PI / 2)
			},
			{
				x: center.x + radius * Math.cos(baseAngle - Math.PI / 2 - halfAngle + Math.PI / 2),
				y: center.y + radius * Math.sin(baseAngle - Math.PI / 2 - halfAngle + Math.PI / 2)
			}
		];

		// More direct computation: tangent point angles from center
		const tp1Angle = baseAngle + (Math.PI - halfAngle);
		const tp2Angle = baseAngle - (Math.PI - halfAngle);
		const tp1 = { x: center.x + radius * Math.cos(tp1Angle), y: center.y + radius * Math.sin(tp1Angle) };
		const tp2 = { x: center.x + radius * Math.cos(tp2Angle), y: center.y + radius * Math.sin(tp2Angle) };

		// Check if cursor is near either tangent point
		for (const tp of [tp1, tp2]) {
			const dist = Math.sqrt((x - tp.x) ** 2 + (y - tp.y) ** 2);
			if (dist < threshold * 2) {
				return {
					x: tp.x, y: tp.y,
					constraints: [{ type: 'Tangent', entity_a: fromPointId, entity_b: entity.id }],
					indicator: { type: 'tangent', x: tp.x, y: tp.y, entityId: entity.id }
				};
			}
		}
	}

	return null;
}

/**
 * Detect perpendicular snap from a point to a line.
 * The foot of perpendicular from fromPoint onto the line.
 *
 * @param {number} x - Cursor X
 * @param {number} y - Cursor Y
 * @param {number} fromPointId - Point we're drawing from
 * @param {number} threshold - Snap threshold in sketch units
 * @returns {SnapResult | null}
 */
function detectPerpendicularSnap(x, y, fromPointId, threshold) {
	const positions = getSketchPositions();
	const fromPos = positions.get(fromPointId);
	if (!fromPos) return null;

	const entities = getSketchEntities();

	for (const entity of entities) {
		if (entity.type !== 'Line') continue;
		const p1 = positions.get(entity.start_id);
		const p2 = positions.get(entity.end_id);
		if (!p1 || !p2) continue;

		// Skip if fromPoint is an endpoint of this line
		if (entity.start_id === fromPointId || entity.end_id === fromPointId) continue;

		// Project fromPoint onto the line (unclamped)
		const abx = p2.x - p1.x;
		const aby = p2.y - p1.y;
		const len2 = abx * abx + aby * aby;
		if (len2 < 1e-12) continue;

		const t = ((fromPos.x - p1.x) * abx + (fromPos.y - p1.y) * aby) / len2;
		// Only snap if foot is within the segment
		if (t < 0 || t > 1) continue;

		const footX = p1.x + t * abx;
		const footY = p1.y + t * aby;

		// Check if cursor is near the perpendicular foot
		const dist = Math.sqrt((x - footX) ** 2 + (y - footY) ** 2);
		if (dist < threshold * 2) {
			return {
				x: footX, y: footY,
				constraints: [{ type: 'Perpendicular', entity_a: fromPointId, entity_b: entity.id }],
				indicator: { type: 'perpendicular', x: footX, y: footY, entityId: entity.id }
			};
		}
	}

	return null;
}

/**
 * Collect all snap candidate points within a radius of the cursor.
 * These are shown as faint preview markers to help users discover snap targets.
 *
 * @param {number} x - Cursor sketch X
 * @param {number} y - Cursor sketch Y
 * @param {number} previewRadius - Radius in sketch units to search
 * @returns {Array<{ type: 'origin'|'point'|'midpoint'|'quadrant'|'reference', x: number, y: number, entityId?: number, sourceId?: string }>}
 */
export function collectSnapCandidates(x, y, previewRadius) {
	/** @type {Array<{ type: 'origin'|'point'|'midpoint'|'quadrant'|'reference', x: number, y: number, entityId?: number, sourceId?: string }>} */
	const candidates = [];

	// Origin
	const originDist = Math.sqrt(x * x + y * y);
	if (originDist < previewRadius) {
		candidates.push({ type: 'origin', x: 0, y: 0 });
	}

	// Points
	const positions = getSketchPositions();
	for (const [id, pos] of positions) {
		const dist = Math.sqrt((x - pos.x) ** 2 + (y - pos.y) ** 2);
		if (dist < previewRadius) {
			candidates.push({ type: 'point', x: pos.x, y: pos.y, entityId: id });
		}
	}

	// Midpoints (of lines)
	const entities = getSketchEntities();
	for (const entity of entities) {
		if (entity.type === 'Line') {
			const p1 = positions.get(entity.start_id);
			const p2 = positions.get(entity.end_id);
			if (!p1 || !p2) continue;
			const midX = (p1.x + p2.x) / 2;
			const midY = (p1.y + p2.y) / 2;
			const dist = Math.sqrt((x - midX) ** 2 + (y - midY) ** 2);
			if (dist < previewRadius) {
				candidates.push({ type: 'midpoint', x: midX, y: midY, entityId: entity.id });
			}
		}
	}

	// Quadrants (of circles and arcs)
	const quadrantAngles = [0, Math.PI / 2, Math.PI, 3 * Math.PI / 2];
	for (const entity of entities) {
		if (entity.type !== 'Circle' && entity.type !== 'Arc') continue;
		const center = positions.get(entity.center_id);
		if (!center) continue;

		let radius;
		if (entity.type === 'Circle') {
			radius = entity.radius;
		} else {
			const startPt = positions.get(entity.start_id);
			if (!startPt) continue;
			radius = Math.sqrt((startPt.x - center.x) ** 2 + (startPt.y - center.y) ** 2);
		}

		for (const angle of quadrantAngles) {
			// For arcs, only include quadrant points within the arc's angular range
			if (entity.type === 'Arc') {
				const startPt = positions.get(entity.start_id);
				const endPt = positions.get(entity.end_id);
				if (!startPt || !endPt) continue;
				let startAngle = Math.atan2(startPt.y - center.y, startPt.x - center.x);
				let endAngle = Math.atan2(endPt.y - center.y, endPt.x - center.x);
				if (endAngle <= startAngle) endAngle += Math.PI * 2;
				let testAngle = angle;
				while (testAngle < startAngle) testAngle += Math.PI * 2;
				if (testAngle > endAngle) continue;
			}

			const qx = center.x + radius * Math.cos(angle);
			const qy = center.y + radius * Math.sin(angle);
			const dist = Math.sqrt((x - qx) ** 2 + (y - qy) ** 2);
			if (dist < previewRadius) {
				candidates.push({ type: 'quadrant', x: qx, y: qy, entityId: entity.id });
			}
		}
	}

	// Reference points (from inactive sketches)
	const refPoints = getReferenceSnapPoints();
	for (const rp of refPoints) {
		const dist = Math.sqrt((x - rp.x) ** 2 + (y - rp.y) ** 2);
		if (dist < previewRadius) {
			candidates.push({ type: 'reference', x: rp.x, y: rp.y, sourceId: rp.sourceId });
		}
	}

	return candidates;
}
