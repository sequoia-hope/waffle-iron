<script>
	import { T, useThrelte } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import { getSketchMode, setCameraRefs } from '$lib/engine/store.svelte.js';

	const { scene, renderer } = useThrelte();

	let cameraRef = $state(null);
	let controlsRef = $state(null);
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
	const MAX_DISTANCE = 200;

	/**
	 * Handle wheel events for zoom-to-cursor behavior.
	 * @param {WheelEvent} e
	 */
	function onWheel(e) {
		if (!cameraRef || !controlsRef) return;

		// Don't override zoom during sketch mode (let OrbitControls handle it)
		if (sketchActive) return;

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

	// Update store refs whenever they change (e.g. after initial bind)
	$effect(() => {
		if (cameraRef && controlsRef) {
			setCameraRefs(cameraRef, controlsRef);
		}
	});
</script>

<T.PerspectiveCamera
	makeDefault
	position={[5, 5, 5]}
	fov={50}
	near={0.1}
	far={1000}
	bind:ref={cameraRef}
>
	<OrbitControls
		bind:ref={controlsRef}
		enabled={!sketchActive}
		enableDamping
		dampingFactor={0.15}
		enableZoom={sketchActive}
		minDistance={0.05}
		maxDistance={200}
	/>
</T.PerspectiveCamera>
