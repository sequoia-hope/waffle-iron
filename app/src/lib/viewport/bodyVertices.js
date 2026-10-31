import * as THREE from 'three';
import { placementMatrix } from './placement.js';

/** Deduplication epsilon for vertex positions (world units). */
const DEDUP_EPS = 1e-4;

/**
 * Unique topological vertex positions of the given body meshes, in world
 * space. Each edge range's first and last polyline point is a topological
 * vertex. Shared by the vertex overlay (what is drawn and picked) and the
 * sketch snap's body-vertex references (what a sketch point can snap and
 * align to), so the two never disagree about which vertices exist.
 *
 * @param {Array<any>} meshData - store meshes (`edges.vertices` / `edges.ranges`)
 * @param {(bodyId: any) => boolean} isVisible - hidden bodies contribute nothing
 * @returns {Array<{ position: THREE.Vector3, featureId: string, bodyId: any, instancePath: string[] | null }>}
 */
export function extractBodyVertices(meshData, isVisible) {
	const vertices = [];
	const seen = [];

	for (const mesh of meshData) {
		if (!isVisible(mesh.bodyId)) continue;
		if (!mesh.edges || !mesh.edges.vertices || !mesh.edges.ranges) continue;
		const verts = mesh.edges.vertices;
		// The owning instance's placement (identity in a Part) is baked into
		// each position, so every consumer gets world coordinates.
		const placement = placementMatrix(mesh.transform);

		for (const range of mesh.edges.ranges) {
			const si = range.start_index;
			const ei = range.end_index;
			if (ei - si < 2) continue; // Need at least 2 vertices

			const endpoints = [
				new THREE.Vector3(verts[si * 3], verts[si * 3 + 1], verts[si * 3 + 2]).applyMatrix4(placement),
				new THREE.Vector3(verts[(ei - 1) * 3], verts[(ei - 1) * 3 + 1], verts[(ei - 1) * 3 + 2]).applyMatrix4(placement)
			];

			for (const pos of endpoints) {
				const isDup = seen.some(s => s.distanceToSquared(pos) < DEDUP_EPS * DEDUP_EPS);
				if (!isDup) {
					seen.push(pos.clone());
					vertices.push({
						position: pos,
						featureId: mesh.featureId,
						bodyId: mesh.bodyId,
						instancePath: mesh.instancePath ?? (mesh.instanceId ? [mesh.instanceId] : null)
					});
				}
			}
		}
	}
	return vertices;
}
