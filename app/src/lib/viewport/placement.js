/**
 * Assembly instance placement (v4 Phase 3b) for the viewport layers.
 *
 * Every mesh the engine sends for an Assembly tab carries the solved
 * `transform` of the instance that owns it, and EVERY layer that draws that
 * mesh's data — faces, edge polylines, topological vertices — has to apply
 * it. The face layer did from the start; the edge and vertex overlays read
 * the same buffers untransformed, so an assembly rendered every part's edges
 * at the origin (a wheel rim around the bottom bracket, a cassette on the
 * crank spindle). One helper, three consumers.
 */
import * as THREE from 'three';

/**
 * Placement as Threlte props. The quaternion is converted to Euler angles
 * because the `quaternion` prop does not take effect on T.Mesh (Threlte v8
 * quirk); `rotation` does.
 * @param {{ translation_m?: number[], rotation_quat?: number[] } | null | undefined} transform
 * @returns {{ position: number[], rotation: number[] }}
 */
export function placementProps(transform) {
	if (!transform) return { position: [0, 0, 0], rotation: [0, 0, 0] };
	const t = transform.translation_m ?? [0, 0, 0];
	const q = transform.rotation_quat ?? [0, 0, 0, 1];
	const euler = new THREE.Euler().setFromQuaternion(new THREE.Quaternion(q[0], q[1], q[2], q[3]));
	return { position: [t[0], t[1], t[2]], rotation: [euler.x, euler.y, euler.z] };
}

/**
 * Placement as a matrix, for layers that bake transformed positions into
 * one shared geometry (the vertex overlay) or pick in world space.
 * @param {{ translation_m?: number[], rotation_quat?: number[] } | null | undefined} transform
 * @returns {THREE.Matrix4}
 */
export function placementMatrix(transform) {
	const m = new THREE.Matrix4();
	if (!transform) return m;
	const t = transform.translation_m ?? [0, 0, 0];
	const q = transform.rotation_quat ?? [0, 0, 0, 1];
	return m.compose(
		new THREE.Vector3(t[0], t[1], t[2]),
		new THREE.Quaternion(q[0], q[1], q[2], q[3]),
		new THREE.Vector3(1, 1, 1)
	);
}
