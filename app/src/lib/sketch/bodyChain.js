/**
 * Connected chains of BODY edges (model geometry) for project/offset:
 * starting from a hovered edge, walk to edges that share an endpoint AND lie
 * in the same plane parallel to the sketch plane — the predictable "outline
 * loop" a user means when they hover a board outline. Pure module: plain
 * arrays in, indices/polylines out. See /specs/sketch_chain_offset.md
 * (cycle 2, explicit chains).
 */

/** 3D endpoint positions closer than this weld into one chain node (m). */
export const BODY_WELD_TOL = 1e-6;
/**
 * All polyline points of a chained edge must lie within this distance (m),
 * measured along the sketch normal, of the seed edge's plane. Matches
 * projectFace's PLANE_TOL so "what chains" == "what projects faithfully".
 */
export const CHAIN_PLANE_TOL = 1e-5;

const key3 = (x, y, z) =>
	`${Math.round(x / BODY_WELD_TOL)},${Math.round(y / BODY_WELD_TOL)},${Math.round(z / BODY_WELD_TOL)}`;

/** Endpoint world positions of an edge range. */
function rangeEndpoints(verts, range) {
	const si = range.start_index;
	const ei = range.end_index;
	return [
		[verts[si * 3], verts[si * 3 + 1], verts[si * 3 + 2]],
		[verts[(ei - 1) * 3], verts[(ei - 1) * 3 + 1], verts[(ei - 1) * 3 + 2]],
	];
}

/** Offset of a point along the sketch normal (plane-parallel coordinate). */
const alongNormal = (p, n) => p[0] * n[0] + p[1] * n[1] + p[2] * n[2];

/**
 * Find the connected edge chain containing `seedIndex` within one mesh.
 *
 * @param {{ edges: { vertices: Float32Array, ranges: Array<object> } }} mesh
 * @param {number} seedIndex - index into mesh.edges.ranges
 * @param {[number,number,number]} sketchNormal - unit sketch-plane normal
 * @returns {{ indices: number[], closed: boolean }}
 */
export function findBodyEdgeChain(mesh, seedIndex, sketchNormal) {
	const verts = mesh?.edges?.vertices;
	const ranges = mesh?.edges?.ranges;
	if (!verts || !ranges || !ranges[seedIndex]) return { indices: [], closed: false };

	// Plane gate: every point of the seed edge defines the reference offset;
	// candidate edges must keep ALL their points within CHAIN_PLANE_TOL of it.
	const seed = ranges[seedIndex];
	let ref = 0;
	for (let k = seed.start_index; k < seed.end_index; k++) {
		ref += alongNormal([verts[k * 3], verts[k * 3 + 1], verts[k * 3 + 2]], sketchNormal);
	}
	ref /= seed.end_index - seed.start_index;

	const inPlane = (range) => {
		for (let k = range.start_index; k < range.end_index; k++) {
			const d = alongNormal([verts[k * 3], verts[k * 3 + 1], verts[k * 3 + 2]], sketchNormal) - ref;
			if (Math.abs(d) > CHAIN_PLANE_TOL) return false;
		}
		return true;
	};

	// Node map over welded endpoints of the in-plane edges.
	/** @type {Map<string, number[]>} node key → range indices touching it */
	const byNode = new Map();
	const eligible = [];
	for (let i = 0; i < ranges.length; i++) {
		if (ranges[i].end_index - ranges[i].start_index < 2) continue;
		if (!inPlane(ranges[i])) continue;
		eligible.push(i);
		for (const p of rangeEndpoints(verts, ranges[i])) {
			const k = key3(p[0], p[1], p[2]);
			if (!byNode.has(k)) byNode.set(k, []);
			byNode.get(k).push(i);
		}
	}
	if (!eligible.includes(seedIndex)) return { indices: [seedIndex], closed: false };

	const visited = new Set([seedIndex]);
	const queue = [seedIndex];
	while (queue.length) {
		const i = queue.pop();
		for (const p of rangeEndpoints(verts, ranges[i])) {
			for (const j of byNode.get(key3(p[0], p[1], p[2])) ?? []) {
				if (!visited.has(j)) {
					visited.add(j);
					queue.push(j);
				}
			}
		}
	}

	// Closed iff every welded node the chain touches has exactly 2 members
	// (a closed-polyline single edge is its own closed loop: 1 node, 2 ends).
	let closed = true;
	const nodeCount = new Map();
	for (const i of visited) {
		for (const p of rangeEndpoints(verts, ranges[i])) {
			const k = key3(p[0], p[1], p[2]);
			nodeCount.set(k, (nodeCount.get(k) ?? 0) + 1);
		}
	}
	for (const c of nodeCount.values()) {
		if (c !== 2) {
			closed = false;
			break;
		}
	}
	return { indices: [...visited].sort((a, b) => a - b), closed };
}

/**
 * Sample chain edges into sketch-plane 2D polylines for the ghost preview.
 * @param {{ edges: { vertices: Float32Array, ranges: Array<object> } }} mesh
 * @param {number[]} indices
 * @param {{ origin: {x,y,z}, normal: {x,y,z}, xAxis: {x,y,z}, yAxis: {x,y,z} }} plane - buildSketchPlane output
 * @returns {Array<Array<[number, number]>>}
 */
export function bodyChainPolylines2D(mesh, indices, plane) {
	const verts = mesh?.edges?.vertices;
	const ranges = mesh?.edges?.ranges;
	if (!verts || !ranges) return [];
	const polylines = [];
	for (const i of indices) {
		const r = ranges[i];
		if (!r) continue;
		const poly = [];
		for (let k = r.start_index; k < r.end_index; k++) {
			const rx = verts[k * 3] - plane.origin.x;
			const ry = verts[k * 3 + 1] - plane.origin.y;
			const rz = verts[k * 3 + 2] - plane.origin.z;
			poly.push([
				rx * plane.xAxis.x + ry * plane.xAxis.y + rz * plane.xAxis.z,
				rx * plane.yAxis.x + ry * plane.yAxis.y + rz * plane.yAxis.z,
			]);
		}
		if (poly.length >= 2) polylines.push(poly);
	}
	return polylines;
}
