<script>
	import { T, useThrelte, useTask } from '@threlte/core';
	import * as THREE from 'three';
	import { getSettings } from '$lib/ui/settings.svelte.js';
	import { isHdriEnvironment } from './studio/studioEnvironment.js';

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

	let settings = $derived(getSettings());
	let isStudio = $derived(settings.viewportShading === 'studio');
	let isHdri = $derived(isStudio && isHdriEnvironment(settings.studioEnvironment));

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

<!-- Ambient fill (classic Shaded = 0.4; Studio lets PMREM IBL carry indirect light) -->
<T.AmbientLight intensity={!isStudio ? 0.4 : isHdri ? 0.04 : 0.08} />

<!-- Main key light — pinned over the viewer's right shoulder (gentle fill when real HDRI drives directionality) -->
<T.DirectionalLight bind:ref={keyLight} intensity={!isStudio ? 0.8 : isHdri ? 0.25 : 0.65} castShadow={false} />

<!-- Secondary fill light (opposite shoulder) -->
<T.DirectionalLight bind:ref={fillLight} intensity={!isStudio ? 0.3 : isHdri ? 0.08 : 0.18} />

<!-- Hemisphere for subtle sky/ground gradient (world-oriented, by design) -->
<T.HemisphereLight args={[0x8888cc, 0x444422, !isStudio ? 0.3 : isHdri ? 0.05 : 0.12]} />
