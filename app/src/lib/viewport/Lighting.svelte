<script>
	import { T, useThrelte, useTask } from '@threlte/core';
	import * as THREE from 'three';

	// Lights are aimed in CAMERA space, not world space: the key light must come
	// from up and over the viewer's right shoulder no matter how the model is
	// oriented. A directional light's direction is (position - target); the
	// default target sits at the origin, so writing `position = dir * DIST`
	// makes `dir` the light direction exactly.
	//
	// Camera frame (three.js): +x right, +y up, +z toward the viewer.
	// Key direction derived from the reference highlight: polar 44.6° off the
	// view axis, azimuth 39.1° CCW from +x (yaw 37.4° right, pitch 26.2° up).
	const KEY_DIR = new THREE.Vector3(0.544, 0.442, 0.712).normalize();
	// Fill from the opposite shoulder, slightly below, to keep back faces read-able.
	const FILL_DIR = new THREE.Vector3(-0.6, -0.15, 0.5).normalize();
	const DIST = 100;

	const { camera } = useThrelte();

	/** @type {THREE.DirectionalLight | undefined} */
	let keyLight = $state();
	/** @type {THREE.DirectionalLight | undefined} */
	let fillLight = $state();

	const _v = new THREE.Vector3();

	useTask(() => {
		const cam = camera.current;
		if (!cam) return;
		if (keyLight) {
			keyLight.position.copy(_v.copy(KEY_DIR).applyQuaternion(cam.quaternion)).multiplyScalar(DIST);
		}
		if (fillLight) {
			fillLight.position.copy(_v.copy(FILL_DIR).applyQuaternion(cam.quaternion)).multiplyScalar(DIST);
		}
	});
</script>

<!-- Ambient fill -->
<T.AmbientLight intensity={0.4} />

<!-- Main key light — pinned over the viewer's right shoulder -->
<T.DirectionalLight bind:ref={keyLight} intensity={0.8} castShadow={false} />

<!-- Secondary fill light (opposite shoulder) -->
<T.DirectionalLight bind:ref={fillLight} intensity={0.3} />

<!-- Hemisphere for subtle sky/ground gradient (world-oriented, by design) -->
<T.HemisphereLight args={[0x8888cc, 0x444422, 0.3]} />
