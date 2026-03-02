<script>
	import { T, useThrelte, useTask } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import {
		getSketchMode, setCameraRefs, getSketchPositions, getMeshes,
		getCameraProjection, setViewCubeTransform
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
	let frustumHalf = $state(30);
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
	const MIN_DISTANCE = 0.05;

	/** Maximum camera distance */
	const MAX_DISTANCE = 2000;

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
		_mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
		_mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;

		// Calculate zoom factor from wheel delta
		// Positive deltaY = scroll down = zoom out; negative = zoom in
		const zoomSpeed = 0.001;
		const delta = -e.deltaY * zoomSpeed;
		const zoomFactor = Math.max(0.1, Math.min(10, 1 + delta));

		if (isOrtho()) {
			// Ortho zoom: adjust frustumHalf (inverse of zoom level)
			frustumHalf = Math.max(0.1, Math.min(5000, frustumHalf / zoomFactor));
			updateOrthoFrustum();

			// Dolly the camera in/out to keep distance proportional to frustum.
			// This enables zoom-to-cursor behavior and keeps camera distance
			// meaningful for raycasting and tests.
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
				// No hit — dolly along view direction
				const currentDist = cameraRef.position.distanceTo(controlsRef.target);
				const newDist = Math.max(MIN_DISTANCE, Math.min(MAX_DISTANCE, currentDist / zoomFactor));
				const direction = new THREE.Vector3()
					.subVectors(cameraRef.position, controlsRef.target)
					.normalize();
				cameraRef.position.copy(controlsRef.target).addScaledVector(direction, newDist);
			}

			cameraRef.updateProjectionMatrix();
			controlsRef.update();
			return;
		}

		// Perspective zoom (original logic)
		// Cast a ray from the camera through the mouse position
		_raycaster.setFromCamera(_mouse, cameraRef);

		// Collect all meshes in the scene for intersection testing
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

		// If no mesh hit, project onto the plane passing through the current
		// orbit target, perpendicular to the camera's view direction
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
			// Zoom-to-cursor: lerp both camera and target toward the hit point.
			// The view direction (target - camera) stays parallel but scales by
			// (1 - fraction), so the point under the cursor remains fixed in
			// screen space via perspective division cancellation.
			const fraction = 1 - (1 / zoomFactor);
			cameraRef.position.lerp(hitPoint, fraction);
			controlsRef.target.lerp(hitPoint, fraction);
		} else {
			// No hit — dolly along view direction
			const currentDist = cameraRef.position.distanceTo(controlsRef.target);
			const newDist = Math.max(MIN_DISTANCE, Math.min(MAX_DISTANCE, currentDist / zoomFactor));
			const direction = new THREE.Vector3()
				.subVectors(cameraRef.position, controlsRef.target)
				.normalize();
			cameraRef.position.copy(controlsRef.target).addScaledVector(direction, newDist);
		}

		// Clamp camera-to-target distance
		const dist = cameraRef.position.distanceTo(controlsRef.target);
		if (dist < MIN_DISTANCE || dist > MAX_DISTANCE) {
			const clampedDist = Math.max(MIN_DISTANCE, Math.min(MAX_DISTANCE, dist));
			const dir = new THREE.Vector3()
				.subVectors(cameraRef.position, controlsRef.target)
				.normalize();
			cameraRef.position.copy(controlsRef.target).addScaledVector(dir, clampedDist);
		}

		cameraRef.updateProjectionMatrix();
		controlsRef.update();
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
	}

	/**
	 * Snap camera to a standard view direction.
	 * @param {string} viewName
	 */
	function snapToView(viewName) {
		const view = standardViews[viewName];
		if (!view || !cameraRef) return;

		const target = controlsRef ? controlsRef.target.clone() : new THREE.Vector3(0, 0, 0);
		const dist = cameraRef.position.distanceTo(target) || 10;

		const newPos = new THREE.Vector3(...view.pos).normalize().multiplyScalar(dist);
		newPos.add(target);
		cameraRef.position.copy(newPos);
		cameraRef.up.set(...view.up);
		cameraRef.lookAt(target);
		cameraRef.updateProjectionMatrix();

		if (controlsRef) {
			controlsRef.update();
		}
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
		) || 10;

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
		return () => {
			canvas.removeEventListener('wheel', onWheel);
			ro.disconnect();
			window.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
			window.removeEventListener('waffle-snap-view-and-fit', /** @type {EventListener} */ (onSnapViewAndFit));
			window.removeEventListener('waffle-align-to-plane', /** @type {EventListener} */ (onAlignToPlane));
			window.removeEventListener('waffle-zoom-to-face', /** @type {EventListener} */ (onZoomToFace));
			window.removeEventListener('waffle-fit-all', onFitAll);
			window.removeEventListener('waffle-camera-projection-changed', /** @type {EventListener} */ (onProjectionChanged));
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
			ONE: -1,                        // Single finger = select/sketch
			TWO: THREE.TOUCH.DOLLY_ROTATE   // Two fingers = pinch zoom + rotate
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
		if (maxExtent < 0.01) return;

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
		position={[30, 30, 30]}
		fov={50}
		near={0.1}
		far={5000}
		bind:ref={cameraRef}
	>
		<OrbitControls
			bind:ref={controlsRef}
			enableDamping
			dampingFactor={0.25}
			rotateSpeed={1.0}
			enableZoom={false}
			minDistance={0.05}
			maxDistance={2000}
		/>
	</T.PerspectiveCamera>
{:else}
	<T.OrthographicCamera
		makeDefault
		position={[30, 30, 30]}
		near={0.1}
		far={5000}
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
			minDistance={0.05}
			maxDistance={2000}
		/>
	</T.OrthographicCamera>
{/if}
