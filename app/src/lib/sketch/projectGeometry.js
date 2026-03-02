import * as THREE from 'three';

/**
 * Project 3D edge vertices onto a sketch plane, returning 2D sketch coordinates.
 *
 * @param {Float32Array} vertices - All edge vertices for the mesh
 * @param {{ start_index: number, end_index: number }} range
 * @param {{ origin: THREE.Vector3, normal: THREE.Vector3, xAxis: THREE.Vector3, yAxis: THREE.Vector3 }} plane
 * @returns {Array<{ x: number, y: number }>}
 */
export function projectEdgeToSketch(vertices, range, plane) {
	const points = [];
	for (let i = range.start_index * 3; i < range.end_index * 3; i += 3) {
		const world = new THREE.Vector3(vertices[i], vertices[i + 1], vertices[i + 2]);
		const rel = world.clone().sub(plane.origin);
		const along = rel.dot(plane.normal);
		const projected = world.clone().addScaledVector(plane.normal, -along);
		const pRel = projected.clone().sub(plane.origin);
		points.push({ x: pRel.dot(plane.xAxis), y: pRel.dot(plane.yAxis) });
	}
	return points;
}

/**
 * Project a closed boundary (array of [x,y,z]) to sketch 2D.
 *
 * @param {Array<[number, number, number]>} boundary
 * @param {{ origin: THREE.Vector3, normal: THREE.Vector3, xAxis: THREE.Vector3, yAxis: THREE.Vector3 }} plane
 * @returns {Array<{ x: number, y: number }>}
 */
export function projectBoundaryToSketch(boundary, plane) {
	return boundary.map(([x, y, z]) => {
		const world = new THREE.Vector3(x, y, z);
		const rel = world.clone().sub(plane.origin);
		const along = rel.dot(plane.normal);
		const projected = world.clone().addScaledVector(plane.normal, -along);
		const pRel = projected.clone().sub(plane.origin);
		return { x: pRel.dot(plane.xAxis), y: pRel.dot(plane.yAxis) };
	});
}

/**
 * Simplify a polyline by collapsing points closer than `tolerance`.
 *
 * @param {Array<{ x: number, y: number }>} points
 * @param {number} [tolerance=0.01]
 * @returns {Array<{ x: number, y: number }>}
 */
export function simplifyPolyline(points, tolerance = 0.00001) {
	if (points.length < 2) return points;
	const result = [points[0]];
	for (let i = 1; i < points.length; i++) {
		const prev = result[result.length - 1];
		const dx = points[i].x - prev.x;
		const dy = points[i].y - prev.y;
		if (Math.sqrt(dx * dx + dy * dy) > tolerance) {
			result.push(points[i]);
		}
	}
	return result;
}
