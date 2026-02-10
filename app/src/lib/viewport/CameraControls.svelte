<script>
	import { T, useThrelte } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import { getSketchMode } from '$lib/engine/store.svelte.js';

	const { scene } = useThrelte();

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
			window.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
			window.removeEventListener('waffle-align-to-plane', /** @type {EventListener} */ (onAlignToPlane));
		};
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
		minDistance={0.5}
		maxDistance={200}
	/>
</T.PerspectiveCamera>
