/**
 * Pure 2D geometry utility functions for sketch tools.
 *
 * All inputs/outputs are plain {x, y} objects in sketch-local coordinates.
 */

/**
 * 2D line-line intersection (infinite lines through p1-p2 and p3-p4).
 * @param {{x:number,y:number}} p1 - Line 1 start
 * @param {{x:number,y:number}} p2 - Line 1 end
 * @param {{x:number,y:number}} p3 - Line 2 start
 * @param {{x:number,y:number}} p4 - Line 2 end
 * @returns {{x:number,y:number}|null} Intersection point, or null if parallel
 */
export function findLineLineIntersection(p1, p2, p3, p4) {
	const d1x = p2.x - p1.x;
	const d1y = p2.y - p1.y;
	const d2x = p4.x - p3.x;
	const d2y = p4.y - p3.y;

	const denom = d1x * d2y - d1y * d2x;
	if (Math.abs(denom) < 1e-12) return null; // parallel or coincident

	const t = ((p3.x - p1.x) * d2y - (p3.y - p1.y) * d2x) / denom;
	return {
		x: p1.x + t * d1x,
		y: p1.y + t * d1y
	};
}

/**
 * Find intersections of an infinite line with a circle.
 * @param {{x:number,y:number}} lineStart
 * @param {{x:number,y:number}} lineEnd
 * @param {{x:number,y:number}} center
 * @param {number} radius
 * @returns {Array<{x:number,y:number}>} 0-2 intersection points
 */
export function findLineCircleIntersections(lineStart, lineEnd, center, radius) {
	const dx = lineEnd.x - lineStart.x;
	const dy = lineEnd.y - lineStart.y;
	const fx = lineStart.x - center.x;
	const fy = lineStart.y - center.y;

	const a = dx * dx + dy * dy;
	if (a < 1e-12) return []; // degenerate line

	const b = 2 * (fx * dx + fy * dy);
	const c = fx * fx + fy * fy - radius * radius;
	const disc = b * b - 4 * a * c;

	if (disc < -1e-10) return [];

	const results = [];
	if (disc < 1e-10) {
		// tangent
		const t = -b / (2 * a);
		results.push({ x: lineStart.x + t * dx, y: lineStart.y + t * dy });
	} else {
		const sqrtDisc = Math.sqrt(disc);
		const t1 = (-b - sqrtDisc) / (2 * a);
		const t2 = (-b + sqrtDisc) / (2 * a);
		results.push({ x: lineStart.x + t1 * dx, y: lineStart.y + t1 * dy });
		results.push({ x: lineStart.x + t2 * dx, y: lineStart.y + t2 * dy });
	}

	return results;
}

/**
 * Normalize angle to [0, 2*PI).
 * @param {number} angle
 * @returns {number}
 */
function normalizeAngle(angle) {
	let a = angle % (2 * Math.PI);
	if (a < 0) a += 2 * Math.PI;
	return a;
}

/**
 * Check if an angle is within an arc's angular span (CCW from startAngle to endAngle).
 * @param {number} angle
 * @param {number} startAngle
 * @param {number} endAngle
 * @returns {boolean}
 */
function isAngleInArc(angle, startAngle, endAngle) {
	const a = normalizeAngle(angle - startAngle);
	let sweep = normalizeAngle(endAngle - startAngle);
	if (sweep < 1e-10) sweep = 2 * Math.PI; // full circle
	return a <= sweep + 1e-10;
}

/**
 * Find intersections of an infinite line with an arc (filtered to arc's angular span).
 * @param {{x:number,y:number}} arcCenter
 * @param {number} arcRadius
 * @param {number} startAngle - Start angle in radians
 * @param {number} endAngle - End angle in radians (CCW sweep)
 * @param {{x:number,y:number}} lineStart
 * @param {{x:number,y:number}} lineEnd
 * @returns {Array<{x:number,y:number}>}
 */
export function findArcLineIntersections(arcCenter, arcRadius, startAngle, endAngle, lineStart, lineEnd) {
	const all = findLineCircleIntersections(lineStart, lineEnd, arcCenter, arcRadius);
	return all.filter(pt => {
		const angle = Math.atan2(pt.y - arcCenter.y, pt.x - arcCenter.x);
		return isAngleInArc(angle, startAngle, endAngle);
	});
}

/**
 * Distance from a point to a line segment.
 * @param {number} px - Point X
 * @param {number} py - Point Y
 * @param {number} x1 - Segment start X
 * @param {number} y1 - Segment start Y
 * @param {number} x2 - Segment end X
 * @param {number} y2 - Segment end Y
 * @returns {number}
 */
export function distanceToLineSegment(px, py, x1, y1, x2, y2) {
	const dx = x2 - x1;
	const dy = y2 - y1;
	const lenSq = dx * dx + dy * dy;

	if (lenSq < 1e-12) {
		// Degenerate segment (point)
		return Math.sqrt((px - x1) ** 2 + (py - y1) ** 2);
	}

	let t = ((px - x1) * dx + (py - y1) * dy) / lenSq;
	t = Math.max(0, Math.min(1, t));

	const closestX = x1 + t * dx;
	const closestY = y1 + t * dy;
	return Math.sqrt((px - closestX) ** 2 + (py - closestY) ** 2);
}

/**
 * Compute the direction of the angle bisector between two direction vectors.
 * Returns a unit vector pointing along the bisector.
 * @param {{x:number,y:number}} dir1 - First direction (should be unit or near-unit)
 * @param {{x:number,y:number}} dir2 - Second direction (should be unit or near-unit)
 * @returns {{x:number,y:number}} Unit bisector direction
 */
export function angleBisector(dir1, dir2) {
	// Normalize inputs
	const len1 = Math.sqrt(dir1.x * dir1.x + dir1.y * dir1.y);
	const len2 = Math.sqrt(dir2.x * dir2.x + dir2.y * dir2.y);
	if (len1 < 1e-12 || len2 < 1e-12) return { x: 1, y: 0 };

	const n1x = dir1.x / len1, n1y = dir1.y / len1;
	const n2x = dir2.x / len2, n2y = dir2.y / len2;

	const bx = n1x + n2x;
	const by = n1y + n2y;
	const blen = Math.sqrt(bx * bx + by * by);

	if (blen < 1e-12) {
		// Anti-parallel: bisector is perpendicular to either direction
		return { x: -n1y, y: n1x };
	}

	return { x: bx / blen, y: by / blen };
}

/**
 * Find the perpendicular foot from a point onto an infinite line.
 * @param {{x:number,y:number}} point
 * @param {{x:number,y:number}} lineStart
 * @param {{x:number,y:number}} lineEnd
 * @returns {{x:number,y:number}} The foot point
 */
export function perpendicularFoot(point, lineStart, lineEnd) {
	const dx = lineEnd.x - lineStart.x;
	const dy = lineEnd.y - lineStart.y;
	const lenSq = dx * dx + dy * dy;

	if (lenSq < 1e-12) return { x: lineStart.x, y: lineStart.y };

	const t = ((point.x - lineStart.x) * dx + (point.y - lineStart.y) * dy) / lenSq;
	return {
		x: lineStart.x + t * dx,
		y: lineStart.y + t * dy
	};
}

/**
 * Compute parameter t of a point projected onto a line segment [p1, p2].
 * Returns value in [0, 1] if the projection is on the segment.
 * @param {{x:number,y:number}} point
 * @param {{x:number,y:number}} p1
 * @param {{x:number,y:number}} p2
 * @returns {number}
 */
export function parameterOnSegment(point, p1, p2) {
	const dx = p2.x - p1.x;
	const dy = p2.y - p1.y;
	const lenSq = dx * dx + dy * dy;
	if (lenSq < 1e-12) return 0;
	return ((point.x - p1.x) * dx + (point.y - p1.y) * dy) / lenSq;
}
