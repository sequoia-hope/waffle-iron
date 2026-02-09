<script>
	import { T, useThrelte } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import { onMount } from 'svelte';
	import * as THREE from 'three';

	const { scene } = useThrelte();

	let cameraRef = $state(null);
	let controlsRef = $state(null);

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

		window.addEventListener('keydown', onKeyDown);
		window.addEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
		return () => {
			window.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('waffle-snap-view', /** @type {EventListener} */ (onSnapView));
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
		enableDamping
		dampingFactor={0.15}
		minDistance={0.5}
		maxDistance={200}
	/>
</T.PerspectiveCamera>
