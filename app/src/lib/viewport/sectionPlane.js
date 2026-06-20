/**
 * Build the THREE.Plane used to clip the model for the capped section view.
 *
 * three.js clipping keeps the half-space on the side the plane normal points
 * AWAY from (a fragment is clipped when it is on the negative side, i.e.
 * normal·p + constant < 0). We orient the clip normal along the section
 * plane normal (optionally flipped), shift the cut along the normal by
 * `offset`, and build the plane through that shifted origin.
 */
import * as THREE from 'three';

/**
 * @param {{ origin: [number,number,number], normal: [number,number,number] } | null} planeState
 * @param {boolean} flipped
 * @param {number} offset
 * @returns {THREE.Plane | null}
 */
export function buildSectionClipPlane(planeState, flipped = false, offset = 0) {
	if (!planeState) return null;
	const n = new THREE.Vector3(
		planeState.normal[0],
		planeState.normal[1],
		planeState.normal[2]
	).normalize();
	if (n.lengthSq() < 1e-12) return null;
	if (flipped) n.negate();

	// Cut point: plane origin shifted along the section normal by `offset`.
	// (Use the un-flipped normal direction for offset so the slider always
	// moves the cut the same physical direction regardless of which half is kept.)
	const origin = new THREE.Vector3(
		planeState.origin[0],
		planeState.origin[1],
		planeState.origin[2]
	);
	const shiftDir = new THREE.Vector3(
		planeState.normal[0],
		planeState.normal[1],
		planeState.normal[2]
	).normalize();
	const cutPoint = origin.addScaledVector(shiftDir, offset);

	return new THREE.Plane().setFromNormalAndCoplanarPoint(n, cutPoint);
}
