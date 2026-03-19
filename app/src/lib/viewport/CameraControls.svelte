<script>
	import { T, useThrelte, useTask } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import {
		getSketchMode, setCameraRefs, getSketchPositions, getMeshes,
		getCameraProjection, setViewCubeTransform, setTwoFingerActive
	} from '$lib/engine/store.svelte.js';

	const { scene, renderer, camera } = useThrelte();

	// --- Orbit-past-poles fix: quaternion-based rotation ---
	// Problem: OrbitControls uses spherical coordinates internally. Its update()
	// calls _spherical.setFromVector3() each frame, which uses Math.acos(y/r).
	// Since acos returns [0, PI], phi is silently clamped — the camera gets stuck
	// at the poles and any spherical-based workaround has theta discontinuities.
	//
	// Fix: Replace OrbitControls.update() with quaternion-based rotation.
	// Mouse/touch input still feeds _sphericalDelta (theta, phi), but we interpret
	// those as rotation angles applied via quaternions instead of spherical coords.
	// This eliminates the pole singularity entirely — the camera orbits smoothly
	// in any direction, and camera.up is rotated alongside the position to keep
	// lookAt() stable.

	// No-op makeSafe — safety measure in case any code path still calls it
	THREE.Spherical.prototype.makeSafe = function () { return this; };

	/**
	 * Replace OrbitControls.update() with quaternion rotation.
	 * Input handlers (_handleMouseMoveRotate, etc.) still set _sphericalDelta.
	 * We read those deltas and apply them as quaternion rotations to the camera
	 * offset and up vector, bypassing the spherical coordinate system entirely.
	 * @param {object} controls - OrbitControls instance
	 * @param {THREE.Camera} camera
	 */
	function applyQuaternionOrbitPatch(controls, camera) {
		// Reusable objects — allocated once, reused every frame
		const _offset = new THREE.Vector3();
		const _quat = new THREE.Quaternion();
		const _quatTheta = new THREE.Quaternion();
		const _quatPhi = new THREE.Quaternion();
		const _right = new THREE.Vector3();
		const _lastPosition = new THREE.Vector3().copy(camera.position);
		const _lastQuaternion = new THREE.Quaternion().copy(camera.quaternion);
		const EPS = 1e-10;

		controls.update = function (_deltaTime) {
			// --- Auto-rotate ---
			if (this.autoRotate && this.state === -1 /* STATE.NONE */) {
				this._rotateLeft(this._getAutoRotationAngle(_deltaTime));
			}

			// --- Compute damped rotation amounts ---
			let dTheta, dPhi;
			if (this.enableDamping) {
				dTheta = this._sphericalDelta.theta * this.dampingFactor;
				dPhi = this._sphericalDelta.phi * this.dampingFactor;
			} else {
				dTheta = this._sphericalDelta.theta;
				dPhi = this._sphericalDelta.phi;
			}

			// --- Camera offset from orbit target ---
			_offset.copy(camera.position).sub(this.target);

			// --- Apply rotation as quaternion ---
			if (Math.abs(dTheta) > EPS || Math.abs(dPhi) > EPS) {
				// Right axis: perpendicular to camera up and look direction
				_right.crossVectors(camera.up, _offset).normalize();
				if (_right.lengthSq() < 1e-6) {
					// Degenerate: offset parallel to up — pick arbitrary perpendicular
					_right.crossVectors(new THREE.Vector3(0, 0, 1), _offset).normalize();
					if (_right.lengthSq() < 1e-6) _right.set(1, 0, 0);
				}

				// Theta (horizontal): rotate around camera's up vector
				// Phi (vertical): rotate around the right vector
				_quatTheta.setFromAxisAngle(camera.up, dTheta);
				_quatPhi.setFromAxisAngle(_right, dPhi);
				_quat.copy(_quatTheta).multiply(_quatPhi);

				// Rotate both the offset and the up vector together.
				// This keeps camera.up perpendicular to the look direction,
				// preventing the lookAt() flip at the poles.
				_offset.applyQuaternion(_quat);
				camera.up.applyQuaternion(_quat).normalize();
			}

			// --- Clamp distance ---
			const dist = _offset.length();
			const clampedDist = Math.max(this.minDistance, Math.min(this.maxDistance, dist));
			if (Math.abs(dist - clampedDist) > EPS) {
				_offset.normalize().multiplyScalar(clampedDist);
			}

			// --- Apply pan ---
			if (this.enableDamping) {
				this.target.addScaledVector(this._panOffset, this.dampingFactor);
			} else {
				this.target.add(this._panOffset);
			}

			// --- Clamp target radius ---
			this.target.sub(this.cursor);
			this.target.clampLength(this.minTargetRadius, this.maxTargetRadius);
			this.target.add(this.cursor);

			// --- Update camera ---
			camera.position.copy(this.target).add(_offset);
			camera.lookAt(this.target);

			// --- Damping decay ---
			if (this.enableDamping) {
				this._sphericalDelta.theta *= (1 - this.dampingFactor);
				this._sphericalDelta.phi *= (1 - this.dampingFactor);
				this._panOffset.multiplyScalar(1 - this.dampingFactor);
			} else {
				this._sphericalDelta.set(0, 0, 0);
				this._panOffset.set(0, 0, 0);
			}

			// --- Change detection — notify Threlte to invalidate viewport ---
			if (_lastPosition.distanceToSquared(camera.position) > EPS ||
				8 * (1 - _lastQuaternion.dot(camera.quaternion)) > EPS) {
				this.dispatchEvent({ type: 'change' });
				_lastPosition.copy(camera.position);
				_lastQuaternion.copy(camera.quaternion);
				return true;
			}
			return false;
		};
	}

	let cameraRef = $state(null);
	let controlsRef = $state(null);
	let hasAutoFitForMesh = false;
	let sketchActive = $derived(getSketchMode()?.active ?? false);
	let projection = $derived(getCameraProjection());

	// Ortho frustum state
	let frustumHalf = $state(0.03);
	let aspect = $state(1);

	// Saved camera state for projection switches
	let savedCameraState = null;

	const standardViews = {
		front:  { pos: [0, 0, 1],  up: [0, 1, 0] },
		back:   { pos: [0, 0, -1], up: [0, 1, 0] },
		top:    { pos: [0, 1, 0],  up: [0, 0, -1] },
		bottom: { pos: [0, -1, 0], up: [0, 0, 1] },
		left:   { pos: [-1, 0, 0], up: [0, 1, 0] },
		right:  { pos: [1, 0, 0],  up: [0, 1, 0] },
		iso:    { pos: [1, 1, 1],  up: [0, 1, 0] }
	};

	// Reusable THREE objects for zoom-to-cursor (avoid per-frame allocations)
	const _raycaster = new THREE.Raycaster();
	const _mouse = new THREE.Vector2();
	const _plane = new THREE.Plane();
	const _planeIntersect = new THREE.Vector3();

	/** Minimum camera distance to prevent zooming through objects */
	const MIN_DISTANCE = 0.00005;

	/** Maximum camera distance (dynamically updated by updateClippingPlanes) */
	let maxDistance = 2;

	// --- Cached scene AABB for clipping plane updates ---
	let cachedSceneBox = new THREE.Box3();
	let cachedSceneSphere = new THREE.Sphere();
	let cachedMeshCount = -1;
	let sceneBBoxValid = false;

	// --- Two-finger touch gesture constants ---
	const TOUCH_TWIST_SPEED = 1.5;
	const MIN_PINCH_DISTANCE_PX = 10;

	// --- Two-finger touch gesture state ---
	/** @type {Array<{id: number, x: number, y: number}>} */
	let activePointers = [];
	/** @type {{x: number, y: number} | null} */
	let prevMidpoint = null;
	/** @type {number | null} */
	let prevDistance = null;
	/** @type {number | null} */
	let prevAngle = null;
	let isTwoFingerActive = false;

	/** @returns {boolean} */
	function isOrtho() {
		return cameraRef && /** @type {any} */ (cameraRef).isOrthographicCamera;
	}

	/**
	 * Update ortho camera frustum from current frustumHalf and aspect.
	 */
	function updateOrthoFrustum() {
		if (!cameraRef || !isOrtho()) return;
		const cam = /** @type {THREE.OrthographicCamera} */ (cameraRef);
		cam.left = -frustumHalf * aspect;
		cam.right = frustumHalf * aspect;
		cam.top = frustumHalf;
		cam.bottom = -frustumHalf;
		cam.updateProjectionMatrix();
	}

	/**
	 * Handle wheel events for zoom-to-cursor behavior.
	 * @param {WheelEvent} e
	 */
	function onWheel(e) {
		if (!cameraRef || !controlsRef) return;

		e.preventDefault();
		e.stopPropagation();

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();

		// Convert mouse position to normalized device coordinates (-1 to +1)
		const ndcX = ((e.clientX - rect.left) / rect.width) * 2 - 1;
		const ndcY = -((e.clientY - rect.top) / rect.height) * 2 + 1;

		// Calculate zoom factor from wheel delta
		// Positive deltaY = scroll down = zoom out; negative = zoom in
		const zoomSpeed = 0.001;
		const delta = -e.deltaY * zoomSpeed;
		const zoomFactor = Math.max(0.1, Math.min(10, 1 + delta));

		zoomTowardScreenPoint(zoomFactor, ndcX, ndcY);
		controlsRef.update();
		updateClippingPlanes();
	}

	/**
	 * Zoom toward a screen point. Shared by wheel zoom and pinch zoom.
	 * @param {number} zoomFactor - >1 zooms in, <1 zooms out
	 * @param {number} ndcX - normalized device coordinate X (-1..1)
	 * @param {number} ndcY - normalized device coordinate Y (-1..1)
	 */
	function zoomTowardScreenPoint(zoomFactor, ndcX, ndcY) {
		if (!cameraRef || !controlsRef) return;

		_mouse.x = ndcX;
		_mouse.y = ndcY;

		if (isOrtho()) {
			frustumHalf = Math.max(0.0001, Math.min(maxDistance * 2, frustumHalf / zoomFactor));
			updateOrthoFrustum();

			_raycaster.setFromCamera(_mouse, cameraRef);
			const cameraDir = new THREE.Vector3();
			cameraRef.getWorldDirection(cameraDir);
			_plane.setFromNormalAndCoplanarPoint(cameraDir, controlsRef.target);
			const ray = _raycaster.ray;
			if (ray.intersectPlane(_plane, _planeIntersect)) {
				const fraction = 1 - (1 / zoomFactor);
				cameraRef.position.lerp(_planeIntersect, fraction);
				controlsRef.target.lerp(_planeIntersect, fraction);
			} else {
				const currentDist = cameraRef.position.distanceTo(controlsRef.target);
				const newDist = Math.max(MIN_DISTANCE, Math.min(maxDistance, currentDist / zoomFactor));
				const direction = new THREE.Vector3()
					.subVectors(cameraRef.position, controlsRef.target)
					.normalize();
				cameraRef.position.copy(controlsRef.target).addScaledVector(direction, newDist);
			}

			cameraRef.updateProjectionMatrix();
			return;
		}

		// Perspective: raycast to find zoom target point
		_raycaster.setFromCamera(_mouse, cameraRef);

		/** @type {THREE.Mesh[]} */
		const meshes = [];
		scene.traverse((obj) => {
			if (/** @type {any} */ (obj).isMesh && obj.visible) {
				meshes.push(/** @type {THREE.Mesh} */ (obj));
			}
		});

		let hitPoint = null;
		if (meshes.length > 0) {
			const intersections = _raycaster.intersectObjects(meshes, false);
			if (intersections.length > 0) {
				hitPoint = intersections[0].point;
			}
		}

		if (!hitPoint) {
			const cameraDir = new THREE.Vector3();
			cameraRef.getWorldDirection(cameraDir);
			_plane.setFromNormalAndCoplanarPoint(cameraDir, controlsRef.target);
			const ray = _raycaster.ray;
			if (ray.intersectPlane(_plane, _planeIntersect)) {
				hitPoint = _planeIntersect;
			}
		}

		if (hitPoint) {
			const fraction = 1 - (1 / zoomFactor);
			cameraRef.position.lerp(hitPoint, fraction);
			controlsRef.target.lerp(hitPoint, fraction);
		} else {
			const currentDist = cameraRef.position.distanceTo(controlsRef.target);
			const newDist = Math.max(MIN_DISTANCE, Math.min(maxDistance, currentDist / zoomFactor));
			const direction = new THREE.Vector3()
				.subVectors(cameraRef.position, controlsRef.target)
				.normalize();
			cameraRef.position.copy(controlsRef.target).addScaledVector(direction, newDist);
		}

		// Clamp camera-to-target distance
		const dist = cameraRef.position.distanceTo(controlsRef.target);
		if (dist < MIN_DISTANCE || dist > maxDistance) {
			const clampedDist = Math.max(MIN_DISTANCE, Math.min(maxDistance, dist));
			const dir = new THREE.Vector3()
				.subVectors(cameraRef.position, controlsRef.target)
				.normalize();
			cameraRef.position.copy(controlsRef.target).addScaledVector(dir, clampedDist);
		}

		cameraRef.updateProjectionMatrix();
	}

	// --- Two-finger touch gesture handlers ---

	/** @param {PointerEvent} e */
	function onTouchPointerDown(e) {
		if (e.pointerType !== 'touch') return;
		// Bounds check: only handle touches that start on the canvas
		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		if (e.clientX < rect.left || e.clientX > rect.right ||
			e.clientY < rect.top || e.clientY > rect.bottom) return;
		// Add or update this pointer
		const idx = activePointers.findIndex(p => p.id === e.pointerId);
		if (idx >= 0) {
			activePointers[idx] = { id: e.pointerId, x: e.clientX, y: e.clientY };
		} else {
			activePointers.push({ id: e.pointerId, x: e.clientX, y: e.clientY });
		}

		if (activePointers.length === 2) {
			// Initialize two-finger gesture state
			const [a, b] = activePointers;
			prevMidpoint = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
			prevDistance = Math.hypot(b.x - a.x, b.y - a.y);
			prevAngle = Math.atan2(b.y - a.y, b.x - a.x);
			isTwoFingerActive = true;
			setTwoFingerActive(true);
		}
	}

	/** @param {PointerEvent} e */
	function onTouchPointerMove(e) {
		if (e.pointerType !== 'touch') return;
		// Update tracked pointer position
		const idx = activePointers.findIndex(p => p.id === e.pointerId);
		if (idx >= 0) {
			activePointers[idx] = { id: e.pointerId, x: e.clientX, y: e.clientY };
		}

		if (activePointers.length !== 2 || !isTwoFingerActive) return;
		if (!controlsRef || !cameraRef) return;

		const [a, b] = activePointers;
		const midX = (a.x + b.x) / 2;
		const midY = (a.y + b.y) / 2;
		const dist = Math.hypot(b.x - a.x, b.y - a.y);
		const angle = Math.atan2(b.y - a.y, b.x - a.x);

		// --- Pan: change in midpoint ---
		if (prevMidpoint) {
			const dx = midX - prevMidpoint.x;
			const dy = midY - prevMidpoint.y;
			if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
				// _pan expects pixel deltas; positive = screen-right/down
				controlsRef._pan(dx, dy);
			}
		}

		// --- Zoom: change in finger distance (pinch/spread) ---
		if (prevDistance !== null && dist > MIN_PINCH_DISTANCE_PX && prevDistance > MIN_PINCH_DISTANCE_PX) {
			const zoomFactor = dist / prevDistance;
			if (Math.abs(zoomFactor - 1) > 0.001) {
				const canvas = renderer.domElement;
				const rect = canvas.getBoundingClientRect();
				const ndcX = ((midX - rect.left) / rect.width) * 2 - 1;
				const ndcY = -((midY - rect.top) / rect.height) * 2 + 1;
				zoomTowardScreenPoint(zoomFactor, ndcX, ndcY);
			}
		}

		// --- Rotate: change in angle between fingers (twist) ---
		if (prevAngle !== null) {
			let twistDelta = angle - prevAngle;
			// Normalize to [-PI, PI] to handle atan2 wraparound
			if (twistDelta > Math.PI) twistDelta -= 2 * Math.PI;
			if (twistDelta < -Math.PI) twistDelta += 2 * Math.PI;
			if (Math.abs(twistDelta) > 0.001) {
				controlsRef._rotateLeft(twistDelta * TOUCH_TWIST_SPEED);
			}
		}

		// Apply all accumulated deltas
		controlsRef.update();
		updateClippingPlanes();

		// Update previous state
		prevMidpoint = { x: midX, y: midY };
		prevDistance = dist;
		prevAngle = angle;
	}

	/** @param {PointerEvent} e */
	function onTouchPointerUp(e) {
		if (e.pointerType !== 'touch') return;
		activePointers = activePointers.filter(p => p.id !== e.pointerId);

		if (activePointers.length < 2) {
			prevMidpoint = null;
			prevDistance = null;
			prevAngle = null;
			if (isTwoFingerActive) {
				isTwoFingerActive = false;
				setTwoFingerActive(false);
			}
		}
	}

	/**
	 * Recompute cached scene AABB only when mesh count changes.
	 * @returns {boolean} Whether the AABB is valid (non-empty).
	 */
	function refreshSceneAABB() {
		let meshCount = 0;
		scene.traverse((obj) => {
			if (/** @type {any} */ (obj).isMesh && obj.visible) meshCount++;
		});
		if (meshCount !== cachedMeshCount) {
			cachedMeshCount = meshCount;
			cachedSceneBox.makeEmpty();
			scene.traverse((obj) => {
				if (/** @type {any} */ (obj).isMesh && obj.visible) {
					cachedSceneBox.expandByObject(obj);
				}
			});
			sceneBBoxValid = !cachedSceneBox.isEmpty();
			if (sceneBBoxValid) cachedSceneBox.getBoundingSphere(cachedSceneSphere);
		}
		return sceneBBoxValid;
	}

	/**
	 * Refresh scene AABB and update maxDistance + ortho near/far.
	 * Perspective uses fixed near/far with log depth buffer for precision.
	 * Orthographic uses tight near/far computed by projecting the scene
	 * AABB onto the camera's view direction — this prevents z-fighting
	 * while still allowing the camera to be inside the model.
	 * @param {boolean} [forceRefresh=false] - Force AABB recomputation.
	 */
	function updateClippingPlanes(forceRefresh) {
		if (!cameraRef || !scene) return;
		if (forceRefresh) cachedMeshCount = -1;
		if (!refreshSceneAABB()) return;
		maxDistance = Math.max(cachedSceneSphere.radius * 20, 2);

		if (isOrtho()) {
			// Project AABB corners onto camera view direction to find tight near/far
			const cam = /** @type {THREE.OrthographicCamera} */ (cameraRef);
			const viewDir = new THREE.Vector3();
			cam.getWorldDirection(viewDir);
			const camPos = cam.position;

			// Signed distance from camera to each AABB corner along view direction
			let minDist = Infinity;
			let maxDist = -Infinity;
			for (let i = 0; i < 8; i++) {
				const x = (i & 1) ? cachedSceneBox.max.x : cachedSceneBox.min.x;
				const y = (i & 2) ? cachedSceneBox.max.y : cachedSceneBox.min.y;
				const z = (i & 4) ? cachedSceneBox.max.z : cachedSceneBox.min.z;
				const d = (x - camPos.x) * viewDir.x +
				          (y - camPos.y) * viewDir.y +
				          (z - camPos.z) * viewDir.z;
				if (d < minDist) minDist = d;
				if (d > maxDist) maxDist = d;
			}

			// Add padding and ensure minimum range for numerical stability
			const padding = Math.max((maxDist - minDist) * 0.1, 0.01);
			cam.near = minDist - padding;
			cam.far = maxDist + padding;
			cam.updateProjectionMatrix();
		}
	}

	/**
	 * Fit camera to view all visible objects in the scene.
	 */
	function fitAll() {
		if (!cameraRef || !scene) return;

		const box = new THREE.Box3();
		scene.traverse((obj) => {
			if (/** @type {any} */ (obj).isMesh && obj.visible) {
				box.expandByObject(obj);
			}
		});

		if (box.isEmpty()) return;

		const center = box.getCenter(new THREE.Vector3());
		const size = box.getSize(new THREE.Vector3());
		const maxDim = Math.max(size.x, size.y, size.z);

		if (isOrtho()) {
			// Ortho: set frustum to frame the scene
			frustumHalf = maxDim * 1.5 / 2;
			updateOrthoFrustum();

			const direction = new THREE.Vector3()
				.subVectors(cameraRef.position, center)
				.normalize();
			// Keep camera at a reasonable distance even in ortho (for raycasting)
			cameraRef.position.copy(center).addScaledVector(direction, maxDim * 2);
			cameraRef.up.set(0, 1, 0);
			cameraRef.lookAt(center);
		} else {
			const fov = /** @type {THREE.PerspectiveCamera} */ (cameraRef).fov * (Math.PI / 180);
			let distance = maxDim / (2 * Math.tan(fov / 2));
			distance *= 1.5;

			const direction = new THREE.Vector3()
				.subVectors(cameraRef.position, center)
				.normalize();
			cameraRef.position.copy(center).addScaledVector(direction, distance);
			cameraRef.up.set(0, 1, 0);
			cameraRef.lookAt(center);
		}

		if (controlsRef) {
			controlsRef.target.copy(center);
			controlsRef.update();
		}

		updateClippingPlanes(true);
	}

	/**
	 * Snap camera to a standard view direction.
	 * @param {string} viewName
	 */
	function snapToView(viewName) {
		const view = standardViews[viewName];
		if (!view || !cameraRef) return;

		const target = controlsRef ? controlsRef.target.clone() : new THREE.Vector3(0, 0, 0);
		const dist = cameraRef.position.distanceTo(target) || 0.01;

		const newPos = new THREE.Vector3(...view.pos).normalize().multiplyScalar(dist);
		newPos.add(target);
		cameraRef.position.copy(newPos);
		cameraRef.up.set(...view.up);
		cameraRef.lookAt(target);
		cameraRef.updateProjectionMatrix();

		if (controlsRef) {
			controlsRef.update();
		}
		updateClippingPlanes();
	}

	/**
	 * Snap to a view and then fit all geometry.
	 * @param {string} viewName
	 */
	function snapToViewAndFit(viewName) {
		snapToView(viewName);
		fitAll();
	}

	/**
	 * Align camera to look face-on at a sketch plane.
	 * @param {[number, number, number]} origin
	 * @param {[number, number, number]} normal
	 */
	function alignToPlane(origin, normal) {
		if (!cameraRef) return;

		const n = new THREE.Vector3(normal[0], normal[1], normal[2]).normalize();
		const o = new THREE.Vector3(origin[0], origin[1], origin[2]);
		const dist = cameraRef.position.distanceTo(
			controlsRef ? controlsRef.target.clone() : o
		) || 0.01;

		// Position camera along the normal direction
		const newPos = o.clone().addScaledVector(n, dist);
		cameraRef.position.copy(newPos);

		// Choose an appropriate up vector (perpendicular to normal)
		const worldUp = new THREE.Vector3(0, 1, 0);
		if (Math.abs(n.dot(worldUp)) > 0.99) {
			cameraRef.up.set(0, 0, -Math.sign(n.y));
		} else {
			cameraRef.up.copy(worldUp);
		}

		cameraRef.lookAt(o);
		cameraRef.updateProjectionMatrix();

		if (controlsRef) {
			controlsRef.target.copy(o);
			controlsRef.update();
		}
		updateClippingPlanes();
	}

	/**
	 * Zoom camera to center on a face.
	 * @param {{ center: number[], normal: number[], size: number }} detail
	 */
	function zoomToFace(detail) {
		if (!cameraRef) return;

		const center = new THREE.Vector3(detail.center[0], detail.center[1], detail.center[2]);
		const normal = new THREE.Vector3(detail.normal[0], detail.normal[1], detail.normal[2]).normalize();
		const size = detail.size;

		// Choose an appropriate up vector (perpendicular to normal)
		const worldUp = new THREE.Vector3(0, 1, 0);
		let up;
		if (Math.abs(normal.dot(worldUp)) > 0.99) {
			up = new THREE.Vector3(0, 0, -Math.sign(normal.y));
		} else {
			up = worldUp.clone();
		}

		if (isOrtho()) {
			frustumHalf = size * 1.5 / 2;
			updateOrthoFrustum();
			const dist = size * 2;
			cameraRef.position.copy(center).addScaledVector(normal, dist);
		} else {
			const fov = /** @type {THREE.PerspectiveCamera} */ (cameraRef).fov * (Math.PI / 180);
			const dist = (size * 1.5) / (2 * Math.tan(fov / 2));
			cameraRef.position.copy(center).addScaledVector(normal, dist);
		}

		cameraRef.up.copy(up);
		cameraRef.lookAt(center);
		cameraRef.updateProjectionMatrix();

		if (controlsRef) {
			controlsRef.target.copy(center);
			controlsRef.update();
		}
		updateClippingPlanes();
	}

	/**
	 * Handle projection switch by saving/restoring camera state.
	 */
	function handleProjectionChanged() {
		if (!cameraRef || !controlsRef) return;

		// Save current state from the old camera (which is about to be replaced)
		savedCameraState = {
			position: cameraRef.position.clone(),
			target: controlsRef.target.clone(),
			up: cameraRef.up.clone(),
			distance: cameraRef.position.distanceTo(controlsRef.target),
		};

		// If switching from perspective to ortho, compute frustumHalf
		if (getCameraProjection() === 'orthographic' && /** @type {any} */ (cameraRef).isPerspectiveCamera) {
			const fov = /** @type {THREE.PerspectiveCamera} */ (cameraRef).fov * (Math.PI / 180);
			frustumHalf = savedCameraState.distance * Math.tan(fov / 2);
		}
	}

	// Sync view cube transform each frame
	useTask(
		'viewcube-sync',
		() => {
			if (!camera.current) return;
			const q = camera.current.quaternion.clone().invert();
			const m = new THREE.Matrix4().makeRotationFromQuaternion(q);
			const e = m.elements;
			const css = `matrix3d(${e[0]},${e[1]},${e[2]},${e[3]},${e[4]},${e[5]},${e[6]},${e[7]},${e[8]},${e[9]},${e[10]},${e[11]},${e[12]},${e[13]},${e[14]},${e[15]})`;
			setViewCubeTransform(css);
		},
		{ autoInvalidate: true }
	);

	onMount(() => {
		// Register camera and controls refs in the store
		if (cameraRef && controlsRef) {
			setCameraRefs(cameraRef, controlsRef);
		}

		// Attach zoom-to-cursor wheel handler on the canvas
		const canvas = renderer.domElement;
		canvas.addEventListener('wheel', onWheel, { passive: false });

		// Attach two-finger touch gesture handlers on window (not canvas)
		// because OrbitControls calls setPointerCapture() on the wrapper div,
		// which redirects pointer events away from the canvas during drags.
		window.addEventListener('pointerdown', onTouchPointerDown);
		window.addEventListener('pointermove', onTouchPointerMove);
		window.addEventListener('pointerup', onTouchPointerUp);
		window.addEventListener('pointercancel', onTouchPointerUp);

		// Update aspect ratio on resize
		const ro = new ResizeObserver(() => {
			const w = canvas.clientWidth;
			const h = canvas.clientHeight;
			if (w > 0 && h > 0) {
				aspect = w / h;
				if (isOrtho()) {
					updateOrthoFrustum();
				}
			}
		});
		ro.observe(canvas);

		// Initialize aspect
		if (canvas.clientWidth > 0 && canvas.clientHeight > 0) {
			aspect = canvas.clientWidth / canvas.clientHeight;
		}

		/** @param {KeyboardEvent} e */
		function onKeyDown(e) {
			if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
			if (e.key === 'f' || e.key === 'F') {
				fitAll();
			}
		}

		/** @param {CustomEvent} e */
		function onSnapView(e) {
			snapToView(e.detail.view);
		}

		/** @param {CustomEvent} e */
		function onSnapViewAndFit(e) {
			snapToViewAndFit(e.detail.view);
		}

		/** @param {CustomEvent} e */
		function onAlignToPlane(e) {
			alignToPlane(e.detail.origin, e.detail.normal);
		}

		/** @param {CustomEvent} e */
		function onZoomToFace(e) {
			zoomToFace(e.detail);
		}

		function onFitAll() {
			fitAll();
		}

		/** @param {CustomEvent} e */
		function onProjectionChanged(e) {
			handleProjectionChanged();
		}

		window.addEventListener('keydown', onKeyDown);
		window.addEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
		window.addEventListener('waffle-snap-view-and-fit', /** @type {EventListener} */ (onSnapViewAndFit));
		window.addEventListener('waffle-align-to-plane', /** @type {EventListener} */ (onAlignToPlane));
		window.addEventListener('waffle-zoom-to-face', /** @type {EventListener} */ (onZoomToFace));
		window.addEventListener('waffle-fit-all', onFitAll);
		window.addEventListener('waffle-camera-projection-changed', /** @type {EventListener} */ (onProjectionChanged));

		/** @param {CustomEvent} e */
		function onViewcubeOrbit(e) {
			if (!controlsRef) return;
			const { dx, dy } = e.detail;
			// Convert pixel delta to radians (scale factor tuned for 60px cube)
			const speed = 0.015;
			controlsRef._rotateLeft(dx * speed);
			controlsRef._rotateUp(dy * speed);
			controlsRef.update();
		}
		window.addEventListener('waffle-viewcube-orbit', /** @type {EventListener} */ (onViewcubeOrbit));

		return () => {
			canvas.removeEventListener('wheel', onWheel);
			window.removeEventListener('pointerdown', onTouchPointerDown);
			window.removeEventListener('pointermove', onTouchPointerMove);
			window.removeEventListener('pointerup', onTouchPointerUp);
			window.removeEventListener('pointercancel', onTouchPointerUp);
			ro.disconnect();
			window.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
			window.removeEventListener('waffle-snap-view-and-fit', /** @type {EventListener} */ (onSnapViewAndFit));
			window.removeEventListener('waffle-align-to-plane', /** @type {EventListener} */ (onAlignToPlane));
			window.removeEventListener('waffle-zoom-to-face', /** @type {EventListener} */ (onZoomToFace));
			window.removeEventListener('waffle-fit-all', onFitAll);
			window.removeEventListener('waffle-camera-projection-changed', /** @type {EventListener} */ (onProjectionChanged));
			window.removeEventListener('waffle-viewcube-orbit', /** @type {EventListener} */ (onViewcubeOrbit));
		};
	});

	// Update store refs and apply quaternion orbit patch when controls are ready
	$effect(() => {
		if (cameraRef && controlsRef) {
			setCameraRefs(cameraRef, controlsRef);
			if (!controlsRef._quaternionPatched) {
				applyQuaternionOrbitPatch(controlsRef, cameraRef);
				controlsRef._quaternionPatched = true;
			}

			// Update ortho near/far on every orbit/pan so clipping stays tight
			function onControlsChange() {
				if (isOrtho()) updateClippingPlanes();
			}
			controlsRef.addEventListener('change', onControlsChange);

			// Restore saved camera state after projection switch remounts the camera
			if (savedCameraState) {
				cameraRef.position.copy(savedCameraState.position);
				cameraRef.up.copy(savedCameraState.up);
				controlsRef.target.copy(savedCameraState.target);
				cameraRef.lookAt(savedCameraState.target);
				if (isOrtho()) {
					updateOrthoFrustum();
				}
				cameraRef.updateProjectionMatrix();
				controlsRef.update();
				savedCameraState = null;
			}

			return () => {
				controlsRef.removeEventListener('change', onControlsChange);
			};
		}
	});

	// Onshape-style mouse navigation:
	// Right-click = orbit, Middle-click = pan, Scroll = zoom, Left-click = select/sketch tools.
	// Mapping is identical in both default and sketch modes — left-click behavior
	// is handled by CadModel (selection) vs tools.js (sketch drawing), not OrbitControls.
	$effect(() => {
		if (!controlsRef) return;
		controlsRef.mouseButtons = {
			LEFT: -1,                    // Reserved for select/sketch tools
			MIDDLE: THREE.MOUSE.PAN,     // Middle = pan
			RIGHT: THREE.MOUSE.ROTATE    // Right = orbit
		};
		controlsRef.touches = {
			ONE: THREE.TOUCH.ROTATE,   // Single finger = orbit via quaternion patch
			TWO: -1                    // Disabled — custom two-finger handler below
		};
		if (!sketchActive) {
			// Ensure controls are re-enabled when leaving sketch mode.
			// BoxSelect or other code may have set enabled=false during sketch.
			controlsRef.enabled = true;
		}
		// Force OrbitControls to sync internal state after button remapping
		controlsRef.update();
	});

	// Auto-fit camera when sketch grows beyond visible area (first sketch only)
	$effect(() => {
		if (!sketchActive || !cameraRef || !controlsRef) return;
		const positions = getSketchPositions();
		const hasMeshes = getMeshes().length > 0;
		if (hasMeshes || positions.size < 2) return;

		// Compute sketch AABB
		let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
		for (const pos of positions.values()) {
			if (pos.x < minX) minX = pos.x;
			if (pos.y < minY) minY = pos.y;
			if (pos.x > maxX) maxX = pos.x;
			if (pos.y > maxY) maxY = pos.y;
		}
		const extentX = maxX - minX;
		const extentY = maxY - minY;
		const maxExtent = Math.max(extentX, extentY);
		if (maxExtent < 0.00001) return;

		// Calculate visible range at current camera distance
		const dist = cameraRef.position.distanceTo(controlsRef.target);

		if (isOrtho()) {
			// In ortho, visible height = 2 * frustumHalf
			const visibleHeight = 2 * frustumHalf;
			if (maxExtent > visibleHeight * 0.8) {
				frustumHalf = maxExtent * 1.2 / 2;
				updateOrthoFrustum();
				controlsRef.update();
			}
		} else {
			const fov = /** @type {THREE.PerspectiveCamera} */ (cameraRef).fov * (Math.PI / 180);
			const visibleHeight = 2 * dist * Math.tan(fov / 2);

			// If sketch fills >80% of view, zoom out to fit with 20% padding
			if (maxExtent > visibleHeight * 0.8) {
				const newDist = (maxExtent * 1.2) / (2 * Math.tan(fov / 2));
				const direction = new THREE.Vector3()
					.subVectors(cameraRef.position, controlsRef.target)
					.normalize();
				cameraRef.position.copy(controlsRef.target).addScaledVector(direction, newDist);
				cameraRef.updateProjectionMatrix();
				controlsRef.update();
			}
		}
	});

	// Auto-fit camera when the first 3D mesh appears (e.g. after first extrude).
	// Only fires once — subsequent model changes don't re-fit (respects user's camera).
	$effect(() => {
		const currentMeshes = getMeshes();
		if (!hasAutoFitForMesh && currentMeshes.length > 0 && cameraRef && controlsRef) {
			setTimeout(() => {
				fitAll();
				hasAutoFitForMesh = true;
			}, 50);
		}
	});
</script>

{#if projection === 'perspective'}
	<T.PerspectiveCamera
		makeDefault
		position={[0.03, 0.03, 0.03]}
		fov={50}
		near={1e-4}
		far={1e7}
		bind:ref={cameraRef}
	>
		<OrbitControls
			bind:ref={controlsRef}
			enableDamping
			dampingFactor={0.25}
			rotateSpeed={1.0}
			enableZoom={false}
			minDistance={0.00005}
			maxDistance={maxDistance}
		/>
	</T.PerspectiveCamera>
{:else}
	<T.OrthographicCamera
		makeDefault
		position={[0.03, 0.03, 0.03]}
		near={-1e7}
		far={1e7}
		left={-frustumHalf * aspect}
		right={frustumHalf * aspect}
		top={frustumHalf}
		bottom={-frustumHalf}
		bind:ref={cameraRef}
	>
		<OrbitControls
			bind:ref={controlsRef}
			enableDamping
			dampingFactor={0.25}
			rotateSpeed={1.0}
			enableZoom={false}
			minDistance={0.00005}
			maxDistance={maxDistance}
		/>
	</T.OrthographicCamera>
{/if}
