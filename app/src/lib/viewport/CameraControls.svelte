<script>
	import { T, useThrelte } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import { getSketchMode, setCameraRefs, getSketchPositions, getMeshes } from '$lib/engine/store.svelte.js';

	const { scene, renderer } = useThrelte();

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
	 * @param {THREE.PerspectiveCamera} camera
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
	const _zoomTarget = new THREE.Vector3();
	const _plane = new THREE.Plane();
	const _planeIntersect = new THREE.Vector3();

	/** Interpolation factor: how much of the distance between orbit target and
	 *  zoom-focus point we close per wheel tick (0 = no shift, 1 = snap) */
	const TARGET_LERP_FACTOR = 0.2;

	/** Minimum camera distance to prevent zooming through objects */
	const MIN_DISTANCE = 0.05;

	/** Maximum camera distance */
	const MAX_DISTANCE = 2000;

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

		// Compute new camera distance
		const currentDist = cameraRef.position.distanceTo(controlsRef.target);
		const newDist = Math.max(MIN_DISTANCE, Math.min(MAX_DISTANCE, currentDist / zoomFactor));

		if (hitPoint) {
			// Copy hit point into our reusable vector
			_zoomTarget.copy(hitPoint);

			// Shift the orbit target toward the zoom target point
			controlsRef.target.lerp(_zoomTarget, TARGET_LERP_FACTOR);

			// Clamp orbit target to stay near model — prevents drift into empty space
			const modelBox = new THREE.Box3();
			scene.traverse((obj) => {
				if (/** @type {any} */ (obj).isMesh && obj.visible) modelBox.expandByObject(obj);
			});
			if (!modelBox.isEmpty()) {
				const modelCenter = modelBox.getCenter(new THREE.Vector3());
				const modelSize = modelBox.getSize(new THREE.Vector3());
				const maxExtent = Math.max(modelSize.x, modelSize.y, modelSize.z, 1.0);
				const maxDrift = maxExtent * 2.0;
				const drift = controlsRef.target.distanceTo(modelCenter);
				if (drift > maxDrift) {
					controlsRef.target.lerpVectors(modelCenter, controlsRef.target, maxDrift / drift);
				}
			}
		}

		// Move camera to maintain the new distance from the (possibly shifted) target
		const direction = new THREE.Vector3()
			.subVectors(cameraRef.position, controlsRef.target)
			.normalize();
		cameraRef.position.copy(controlsRef.target).addScaledVector(direction, newDist);

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
		const fov = cameraRef.fov * (Math.PI / 180);
		let distance = maxDim / (2 * Math.tan(fov / 2));
		distance *= 1.5;

		const direction = new THREE.Vector3()
			.subVectors(cameraRef.position, center)
			.normalize();
		cameraRef.position.copy(center).addScaledVector(direction, distance);
		cameraRef.up.set(0, 1, 0);
		cameraRef.lookAt(center);

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

	onMount(() => {
		// Register camera and controls refs in the store
		if (cameraRef && controlsRef) {
			setCameraRefs(cameraRef, controlsRef);
		}

		// Attach zoom-to-cursor wheel handler on the canvas
		const canvas = renderer.domElement;
		canvas.addEventListener('wheel', onWheel, { passive: false });

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
		function onAlignToPlane(e) {
			alignToPlane(e.detail.origin, e.detail.normal);
		}

		window.addEventListener('keydown', onKeyDown);
		window.addEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
		window.addEventListener('waffle-align-to-plane', /** @type {EventListener} */ (onAlignToPlane));
		return () => {
			canvas.removeEventListener('wheel', onWheel);
			window.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
			window.removeEventListener('waffle-align-to-plane', /** @type {EventListener} */ (onAlignToPlane));
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
		const fov = cameraRef.fov * (Math.PI / 180);
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
