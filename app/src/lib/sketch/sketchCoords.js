/**
 * Coordinate projection utilities for sketch plane interaction.
 *
 * Converts between screen coordinates, 3D world space, and 2D sketch-local coordinates.
 */

import * as THREE from 'three';

/**
 * Build a sketch plane coordinate system from origin and normal.
 *
 * @param {[number, number, number]} origin - Plane origin point
 * @param {[number, number, number]} normal - Plane normal vector
 * @returns {{ origin: THREE.Vector3, normal: THREE.Vector3, xAxis: THREE.Vector3, yAxis: THREE.Vector3, plane: THREE.Plane, quaternion: THREE.Quaternion }}
 */
export function buildSketchPlane(origin, normal) {
	const o = new THREE.Vector3(origin[0], origin[1], origin[2]);
	const n = new THREE.Vector3(normal[0], normal[1], normal[2]).normalize();

	// Choose a reference vector not parallel to the normal
	const ref = Math.abs(n.dot(new THREE.Vector3(0, 0, 1))) < 0.99
		? new THREE.Vector3(0, 0, 1)
		: new THREE.Vector3(1, 0, 0);

	const xAxis = new THREE.Vector3().crossVectors(ref, n).normalize();
	const yAxis = new THREE.Vector3().crossVectors(n, xAxis).normalize();

	const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(n, o);

	const quaternion = new THREE.Quaternion();
	quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), n);

	return { origin: o, normal: n, xAxis, yAxis, plane, quaternion };
}

/**
 * Convert screen (pointer event) coordinates to 2D sketch coordinates.
 *
 * @param {{ clientX: number, clientY: number }} event - Pointer event
 * @param {HTMLElement} domElement - The canvas DOM element
 * @param {THREE.Camera} camera - The scene camera
 * @param {{ origin: THREE.Vector3, normal: THREE.Vector3, xAxis: THREE.Vector3, yAxis: THREE.Vector3, plane: THREE.Plane }} sketchPlane - From buildSketchPlane()
 * @returns {{ x: number, y: number, worldPoint: THREE.Vector3 } | null}
 */
export function screenToSketchCoords(event, domElement, camera, sketchPlane) {
	const rect = domElement.getBoundingClientRect();
	const ndcX = ((event.clientX - rect.left) / rect.width) * 2 - 1;
	const ndcY = -((event.clientY - rect.top) / rect.height) * 2 + 1;

	const raycaster = new THREE.Raycaster();
	raycaster.setFromCamera(new THREE.Vector2(ndcX, ndcY), camera);

	const intersection = new THREE.Vector3();
	const hit = raycaster.ray.intersectPlane(sketchPlane.plane, intersection);
	if (!hit) return null;

	// Project 3D intersection to 2D sketch coordinates
	const relative = intersection.clone().sub(sketchPlane.origin);
	const x = relative.dot(sketchPlane.xAxis);
	const y = relative.dot(sketchPlane.yAxis);

	return { x, y, worldPoint: intersection };
}

/**
 * Convert 2D sketch coordinates to 3D world coordinates.
 *
 * @param {number} x - Sketch X coordinate
 * @param {number} y - Sketch Y coordinate
 * @param {{ origin: THREE.Vector3, xAxis: THREE.Vector3, yAxis: THREE.Vector3 }} sketchPlane - From buildSketchPlane()
 * @returns {THREE.Vector3}
 */
export function sketchToWorld(x, y, sketchPlane) {
	return new THREE.Vector3()
		.copy(sketchPlane.origin)
		.addScaledVector(sketchPlane.xAxis, x)
		.addScaledVector(sketchPlane.yAxis, y);
}

/**
 * Convert 2D sketch coordinates to screen pixel coordinates.
 *
 * @param {number} sketchX - Sketch X coordinate
 * @param {number} sketchY - Sketch Y coordinate
 * @param {{ origin: THREE.Vector3, xAxis: THREE.Vector3, yAxis: THREE.Vector3 }} sketchPlane - From buildSketchPlane()
 * @param {THREE.Camera} camera - The scene camera
 * @param {HTMLCanvasElement} canvas - The renderer canvas element
 * @returns {{ x: number, y: number }}
 */
export function sketchToScreen(sketchX, sketchY, sketchPlane, camera, canvas) {
	const world = sketchToWorld(sketchX, sketchY, sketchPlane);
	const ndc = world.project(camera);
	const rect = canvas.getBoundingClientRect();
	return {
		x: rect.left + ((ndc.x + 1) / 2) * rect.width,
		y: rect.top + ((1 - ndc.y) / 2) * rect.height
	};
}
