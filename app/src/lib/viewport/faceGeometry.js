/**
 * Face boundary extraction utilities for ghost preview of face-based regions.
 */

/**
 * Check if two GeomRefs refer to the same entity.
 * Duplicated from store to avoid circular imports.
 * @param {any} a
 * @param {any} b
 * @returns {boolean}
 */
function geomRefEquals(a, b) {
	if (!a || !b) return false;
	return (
		a.kind?.type === b.kind?.type &&
		a.anchor?.type === b.anchor?.type &&
		a.anchor?.feature_id === b.anchor?.feature_id &&
		a.anchor?.plane === b.anchor?.plane &&
		a.anchor?.id === b.anchor?.id &&
		a.selector?.type === b.selector?.type &&
		JSON.stringify(a.selector) === JSON.stringify(b.selector)
	);
}

/**
 * Find the mesh and faceRange matching a geomRef.
 * @param {Array<any>} meshes - Array of mesh data from engine
 * @param {any} geomRef - GeomRef to find
 * @returns {{ mesh: any, range: { start_index: number, end_index: number, geom_ref: any } } | null}
 */
export function findFaceRangeByRef(meshes, geomRef) {
	for (const mesh of meshes) {
		if (!mesh.faceRanges) continue;
		for (const range of mesh.faceRanges) {
			if (geomRefEquals(range.geom_ref, geomRef)) {
				return { mesh, range };
			}
		}
	}
	return null;
}

/**
 * Make an edge key from two vertex indices, order-independent.
 * @param {number} a
 * @param {number} b
 * @returns {string}
 */
function edgeKey(a, b) {
	return a < b ? `${a}_${b}` : `${b}_${a}`;
}

/**
 * Extract the boundary polygon of a mesh face (identified by faceRange)
 * by finding triangle edges that appear only once (boundary edges).
 *
 * @param {any} mesh - { vertices: Float32Array, indices: Uint32Array, faceRanges }
 * @param {{ start_index: number, end_index: number }} range
 * @returns {Array<[number, number, number]>} Ordered boundary vertices (world coords)
 */
export function extractFaceBoundary(mesh, range) {
	const vertices = /** @type {Float32Array} */ (mesh.vertices);
	const indices = /** @type {Uint32Array} */ (mesh.indices);
	if (!vertices || !indices) return [];

	// 1. Collect all edges from triangles in [start_index, end_index)
	// Count occurrences: edges appearing once are boundary, twice are interior
	/** @type {Map<string, { count: number, a: number, b: number }>} */
	const edgeCounts = new Map();

	for (let i = range.start_index; i < range.end_index; i += 3) {
		const i0 = indices[i];
		const i1 = indices[i + 1];
		const i2 = indices[i + 2];

		const triEdges = [
			[i0, i1],
			[i1, i2],
			[i2, i0]
		];

		for (const [a, b] of triEdges) {
			const key = edgeKey(a, b);
			const existing = edgeCounts.get(key);
			if (existing) {
				existing.count++;
			} else {
				edgeCounts.set(key, { count: 1, a, b });
			}
		}
	}

	// 2. Collect boundary edges (count === 1)
	/** @type {Map<number, number[]>} vertex -> adjacent boundary vertices */
	const adjacency = new Map();

	for (const entry of edgeCounts.values()) {
		if (entry.count !== 1) continue;
		const { a, b } = entry;

		let neighborsA = adjacency.get(a);
		if (!neighborsA) { neighborsA = []; adjacency.set(a, neighborsA); }
		neighborsA.push(b);

		let neighborsB = adjacency.get(b);
		if (!neighborsB) { neighborsB = []; adjacency.set(b, neighborsB); }
		neighborsB.push(a);
	}

	if (adjacency.size === 0) return [];

	// 3. Chain boundary edges into an ordered loop
	const visited = new Set();
	const startVertex = /** @type {number} */ (adjacency.keys().next().value);
	const chain = [startVertex];
	visited.add(startVertex);

	let current = startVertex;
	while (true) {
		const neighbors = adjacency.get(current) || [];
		let next = -1;
		for (const n of neighbors) {
			if (!visited.has(n)) {
				next = n;
				break;
			}
		}
		if (next === -1) break;
		chain.push(next);
		visited.add(next);
		current = next;
	}

	// 4. Convert vertex indices to 3D positions
	return chain.map(idx => /** @type {[number, number, number]} */ ([
		vertices[idx * 3],
		vertices[idx * 3 + 1],
		vertices[idx * 3 + 2]
	]));
}
