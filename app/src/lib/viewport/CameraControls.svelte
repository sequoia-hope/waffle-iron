<script>
	import { T, useThrelte } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import { onMount } from 'svelte';
	import * as THREE from 'three';

	const { scene } = useThrelte();

	let cameraRef = $state(null);
	let controlsRef = $state(null);

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

	onMount(() => {
		/** @param {KeyboardEvent} e */
		function onKeyDown(e) {
			if (e.key === 'f' || e.key === 'F') {
				if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
				fitAll();
			}
		}
		window.addEventListener('keydown', onKeyDown);
		return () => window.removeEventListener('keydown', onKeyDown);
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
