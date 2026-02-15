<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import {
		selectRef,
		setHoveredRef,
		getHoveredRef,
		getSelectedRefs,
		geomRefEquals
	} from '$lib/engine/store.svelte.js';
	import { BUILTIN_PLANES, makePlaneRef, PLANE_HALF_SIZE } from '$lib/engine/planes.js';

	// --- Data-driven plane rendering ---

	const planeGeometry = new THREE.PlaneGeometry(PLANE_HALF_SIZE * 2, PLANE_HALF_SIZE * 2);

	// Compute rotation from plane normal using quaternion
	function computeRotation(normal) {
		const quat = new THREE.Quaternion().setFromUnitVectors(
			new THREE.Vector3(0, 0, 1),
			new THREE.Vector3(normal[0], normal[1], normal[2])
		);
		const euler = new THREE.Euler().setFromQuaternion(quat);
		return [euler.x, euler.y, euler.z];
	}

	// Build plane data (materials, refs, rotations)
	const planeData = BUILTIN_PLANES.map((plane) => {
		const resolved = { origin: plane.definition.origin, normal: plane.definition.normal };
		return {
			plane,
			ref: makePlaneRef(plane.id),
			rotation: computeRotation(resolved.normal),
			fillMaterial: new THREE.MeshBasicMaterial({
				color: plane.color,
				transparent: true,
				opacity: 0.02,
				side: THREE.DoubleSide,
				depthWrite: false
			}),
			borderMaterial: new THREE.LineBasicMaterial({
				color: plane.borderColor,
				transparent: true,
				opacity: 0.08
			}),
		};
	});

	/**
	 * Get opacity and color for a datum plane based on hover/selection state.
	 */
	function getPlaneStyle(ref, plane) {
		const selected = getSelectedRefs().some((r) => geomRefEquals(r, ref));
		const hovered = geomRefEquals(getHoveredRef(), ref);

		if (selected) return { opacity: 0.25, color: plane.selectedColor };
		if (hovered) return { opacity: 0.15, color: plane.hoverColor };
		return { opacity: 0.02, color: plane.color };
	}

	// Reactive style derivations
	let styles = $derived(planeData.map((d) => getPlaneStyle(d.ref, d.plane)));

	// Update materials reactively
	$effect(() => {
		for (let i = 0; i < planeData.length; i++) {
			planeData[i].fillMaterial.opacity = styles[i].opacity;
			planeData[i].fillMaterial.color.setHex(styles[i].color);
		}
	});

	// Event handlers
	function handleClick(ref, event) {
		event.stopPropagation();
		const additive = event.nativeEvent?.shiftKey ?? false;
		selectRef(ref, additive);
	}

	function handlePointerEnter(ref, event) {
		if (event) event.stopPropagation();
		setHoveredRef(ref);
	}

	function handlePointerLeave(ref) {
		if (geomRefEquals(getHoveredRef(), ref)) {
			setHoveredRef(null);
		}
	}

	// Border geometry
	function buildPlaneBorder(size) {
		const s = size;
		const pts = new Float32Array([
			-s, -s, 0, s, -s, 0,
			s, -s, 0, s, s, 0,
			s, s, 0, -s, s, 0,
			-s, s, 0, -s, -s, 0
		]);
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(pts, 3));
		return geo;
	}

	const borderGeo = buildPlaneBorder(PLANE_HALF_SIZE);

	// --- Origin Triad (scaled to match plane size) ---

	const axisLength = 20;

	function buildAxisLine(dir, length) {
		const pts = new Float32Array([0, 0, 0, dir[0] * length, dir[1] * length, dir[2] * length]);
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(pts, 3));
		return geo;
	}

	const xAxisGeo = buildAxisLine([1, 0, 0], axisLength);
	const yAxisGeo = buildAxisLine([0, 1, 0], axisLength);
	const zAxisGeo = buildAxisLine([0, 0, 1], axisLength);

	const xAxisMaterial = new THREE.LineBasicMaterial({ color: 0xff4444 });
	const yAxisMaterial = new THREE.LineBasicMaterial({ color: 0x44cc44 });
	const zAxisMaterial = new THREE.LineBasicMaterial({ color: 0x4488ff });

	// Arrowhead cones (scaled proportionally)
	const coneGeo = new THREE.ConeGeometry(0.5, 1.8, 8);

	const xConeMaterial = new THREE.MeshBasicMaterial({ color: 0xff4444 });
	const yConeMaterial = new THREE.MeshBasicMaterial({ color: 0x44cc44 });
	const zConeMaterial = new THREE.MeshBasicMaterial({ color: 0x4488ff });

	// Cone rotations to point along each axis
	const xConeRotation = [0, 0, -Math.PI / 2];
	const yConeRotation = [0, 0, 0];
	const zConeRotation = [Math.PI / 2, 0, 0];

	// Origin sphere
	const originGeo = new THREE.SphereGeometry(0.4, 12, 8);
	const originMaterial = new THREE.MeshBasicMaterial({ color: 0xcccccc });
</script>

<!-- Datum Planes (data-driven) -->
{#each planeData as pd, i (pd.plane.id)}
	<T.Group rotation={pd.rotation}>
		<T.Mesh
			geometry={planeGeometry}
			material={pd.fillMaterial}
			onclick={(e) => handleClick(pd.ref, e)}
			onpointerenter={(e) => handlePointerEnter(pd.ref, e)}
			onpointerleave={() => handlePointerLeave(pd.ref)}
		/>
		<T.LineSegments geometry={borderGeo} material={pd.borderMaterial} />
	</T.Group>
{/each}

<!-- Origin Triad -->
<T.Group>
	<!-- Axes -->
	<T.LineSegments geometry={xAxisGeo} material={xAxisMaterial} />
	<T.LineSegments geometry={yAxisGeo} material={yAxisMaterial} />
	<T.LineSegments geometry={zAxisGeo} material={zAxisMaterial} />

	<!-- Arrowheads -->
	<T.Mesh
		geometry={coneGeo}
		material={xConeMaterial}
		position={[axisLength, 0, 0]}
		rotation={xConeRotation}
	/>
	<T.Mesh
		geometry={coneGeo}
		material={yConeMaterial}
		position={[0, axisLength, 0]}
		rotation={yConeRotation}
	/>
	<T.Mesh
		geometry={coneGeo}
		material={zConeMaterial}
		position={[0, 0, axisLength]}
		rotation={zConeRotation}
	/>

	<!-- Origin point -->
	<T.Mesh geometry={originGeo} material={originMaterial} />
</T.Group>
