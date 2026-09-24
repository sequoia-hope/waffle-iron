<script>
	/**
	 * The "display rotation center" debug marker (Settings → Debug): a small
	 * translucent green sphere at the point an orbit turns about, shown ONLY
	 * while a rotate is in progress.
	 *
	 * The point is `controls.orbitPivot` — re-anchored to whatever is under the
	 * cursor at the start of each rotate — falling back to the look-at target
	 * when nothing was under it. Seeing it is the whole point: the pivot and
	 * the target are different things (that is why an orbit no longer follows
	 * a target that zoom-to-cursor pushed into empty space), and this says
	 * which point the gesture actually picked.
	 *
	 * Constant SCREEN size: the marker is scaled from the view every frame, so
	 * it reads the same on a 3 mm screw and on a 330 m tower.
	 */
	import { T, useTask, useThrelte } from '@threlte/core';
	import * as THREE from 'three';
	import { getCameraRefs, isOrbitActive } from '$lib/engine/store.svelte.js';
	import { getSettings } from '$lib/ui/settings.svelte.js';

	const { invalidate } = useThrelte();

	let enabled = $derived(getSettings().showRotationCenter);
	let orbiting = $derived(isOrbitActive());
	let visible = $derived(enabled && orbiting);

	/** @type {THREE.Mesh | null} */
	let markerRef = $state(null);

	/** Fraction of the view height the sphere spans. */
	const SCREEN_FRACTION = 0.018;

	useTask(
		'rotation-center',
		() => {
			if (!visible || !markerRef) return;
			const { camera, controls } = getCameraRefs();
			if (!camera || !controls) return;
			const at = controls.orbitPivot ?? controls.target;
			markerRef.position.copy(at);
			// Screen-constant radius: half the visible height at the marker's
			// depth, times the fraction above.
			let half;
			if (camera.isOrthographicCamera) {
				half = (camera.top - camera.bottom) / 2 / (camera.zoom || 1);
			} else {
				const d = camera.position.distanceTo(at);
				half = d * Math.tan((camera.fov * Math.PI) / 360);
			}
			const r = Math.max(1e-6, half * SCREEN_FRACTION);
			markerRef.scale.setScalar(r);
			invalidate();
		},
		{ autoInvalidate: false }
	);
</script>

{#if visible}
	<T.Mesh
		bind:ref={markerRef}
		renderOrder={10}
		frustumCulled={false}
		userData={{ waffleType: 'helper' }}
		raycast={() => {}}
	>
		<T.SphereGeometry args={[1, 24, 16]} />
		<T.MeshBasicMaterial
			color={0x33dd55}
			transparent
			opacity={0.45}
			depthTest={false}
			depthWrite={false}
		/>
	</T.Mesh>
{/if}
