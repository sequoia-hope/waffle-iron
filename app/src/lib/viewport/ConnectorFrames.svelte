<script>
	/**
	 * Mate-connector triads (`specs/assembly_connector_frame_resolver.md` §2.5).
	 *
	 * Every connector of the open Assembly tab, drawn where the engine put it:
	 * a short R/G/B triad at the frame's origin, with z (the mate axis) longer
	 * than x and y. Until this existed a connector was invisible — neither its
	 * position nor which way its z pointed could be checked, and `flip` was a
	 * checkbox toggled blind.
	 *
	 * The frames arrive in WORLD coordinates (`ModelUpdated.assembly.connectors`),
	 * so the geometry is built from raw points and the object carries no
	 * transform — no quaternion prop, which Threlte v8 silently ignores.
	 */
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getAssemblyConnectorFrames } from '$lib/engine/store.svelte.js';
	import { AXIS_COLORS } from '$lib/config.js';

	/** Axis lengths in meters. z is the mate axis, so it reads longest. */
	const AXIS_LENGTH = { x_axis: 0.004, y_axis: 0.004, z_axis: 0.009 };
	/** Keyed by the frame's own axis names; values are the shared triad. */
	const AXIS_COLOR = { x_axis: AXIS_COLORS.x, y_axis: AXIS_COLORS.y, z_axis: AXIS_COLORS.z };
	const AXES = ['x_axis', 'y_axis', 'z_axis'];

	let frames = $derived(getAssemblyConnectorFrames());

	/** One LineSegments geometry per axis: a segment per connector. */
	function axisGeometry(list, axis) {
		const points = new Float32Array(list.length * 6);
		const length = AXIS_LENGTH[axis];
		list.forEach((f, i) => {
			const o = f.origin ?? [0, 0, 0];
			const d = f[axis] ?? [0, 0, 0];
			points.set(
				[o[0], o[1], o[2], o[0] + d[0] * length, o[1] + d[1] * length, o[2] + d[2] * length],
				i * 6
			);
		});
		const geometry = new THREE.BufferGeometry();
		geometry.setAttribute('position', new THREE.BufferAttribute(points, 3));
		return geometry;
	}

	// Rebuild on every evaluation; dispose what the previous one made so a
	// long editing session does not leak a geometry per re-solve.
	let previous = [];
	let axisGeometries = $derived.by(() => {
		for (const g of previous) g.dispose();
		previous = frames.length ? AXES.map((axis) => axisGeometry(frames, axis)) : [];
		return previous;
	});

	const materials = AXES.map(
		(axis) =>
			new THREE.LineBasicMaterial({
				color: AXIS_COLOR[axis],
				// Drawn over the model: a connector inside a bore would
				// otherwise be hidden by the part it belongs to.
				depthTest: false,
				transparent: true,
				opacity: 0.95
			})
	);
</script>

{#each axisGeometries as geometry, i (i)}
	<T.LineSegments {geometry} material={materials[i]} renderOrder={10} />
{/each}
