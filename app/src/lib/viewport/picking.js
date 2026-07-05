/**
 * Shared screen-space picking helpers for the viewport overlays.
 *
 * Two responsibilities, both scale-aware so picking behaves identically at any
 * part scale / zoom (Constitution §7 — one rule, no per-mode branching):
 *
 *  - `worldPerPixel`: convert a screen-pixel threshold to the world-space
 *    distance three.js raycasters expect. For the ortho camera this is exact and
 *    depth-independent; for perspective it is evaluated at a given distance.
 *  - occlusion (`faceOccludes` / `faceOccludesPoint`): invariant I2 — an edge or
 *    vertex is hoverable iff no face hit is STRICTLY closer to the camera than
 *    the edge/vertex, beyond a small screen-calibrated depth epsilon. A face
 *    coplanar with or behind the edge/vertex never suppresses it.
 */
import * as THREE from 'three';

const _mouse = new THREE.Vector2();
const _ray = new THREE.Raycaster();

/** Depth epsilon for occlusion, in screen pixels (converted to world per call). */
export const OCCLUSION_DEPTH_EPS_PX = 1.5;

/**
 * World units per screen pixel for the current camera.
 * @param {any} camera
 * @param {number} canvasHeightPx
 * @param {number} [distance] - camera→target distance (perspective only)
 * @returns {number}
 */
export function worldPerPixel(camera, canvasHeightPx, distance) {
	if (!camera || !canvasHeightPx) return 0.01;
	if (camera.isOrthographicCamera) {
		const h = (camera.top - camera.bottom) / (camera.zoom || 1);
		return h / canvasHeightPx;
	}
	if (camera.isPerspectiveCamera) {
		const d = distance ?? camera.position.length();
		const vFov = (camera.fov * Math.PI) / 180;
		return (2 * d * Math.tan(vFov / 2)) / canvasHeightPx;
	}
	return 0.01;
}

/**
 * Distance to the nearest visible face along the ray through a screen pixel, or
 * Infinity if no face is under the cursor. Sets `_ray` as a side effect so the
 * caller can reuse its ray for depth math.
 * @param {any} camera
 * @param {any} renderer
 * @param {number} clientX
 * @param {number} clientY
 * @returns {number}
 */
function nearestFaceDistance(camera, renderer, clientX, clientY) {
	if (!camera || !renderer) return Infinity;
	const rect = renderer.domElement.getBoundingClientRect();
	_mouse.set(
		((clientX - rect.left) / rect.width) * 2 - 1,
		-((clientY - rect.top) / rect.height) * 2 + 1
	);
	_ray.setFromCamera(_mouse, camera);

	const meshObjects = [];
	const scene = camera.parent;
	if (scene) {
		scene.traverse((obj) => {
			if (/** @type {any} */ (obj).isMesh && obj.visible) meshObjects.push(obj);
		});
	}
	const hits = _ray.intersectObjects(meshObjects, false);
	return hits.length > 0 ? hits[0].distance : Infinity;
}

/**
 * True when a face is strictly closer than `hitDistance` at the pixel, so a
 * hover/pick at that depth is occluded (invariant I2). `hitDistance` is an
 * along-ray distance comparable to the face raycast (e.g. a three.js Line
 * intersection distance).
 * @returns {boolean}
 */
export function faceOccludes(camera, renderer, clientX, clientY, hitDistance, epsPx) {
	const faceDist = nearestFaceDistance(camera, renderer, clientX, clientY);
	if (faceDist === Infinity) return false;
	const rect = renderer?.domElement?.getBoundingClientRect();
	const eps = epsPx * worldPerPixel(camera, rect?.height ?? 800, hitDistance);
	return faceDist < hitDistance - eps;
}
