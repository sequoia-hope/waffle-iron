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

	// --- Datum plane GeomRef constants ---

	const datumRefs = {
		XY: { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XY' } },
		XZ: { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'XZ' } },
		YZ: { kind: { type: 'Face' }, anchor: { type: 'DatumPlane', plane: 'YZ' } },
	};

	// Base colors for each plane
	const baseColors = {
		XY: 0x4444aa,
		XZ: 0x44aa44,
		YZ: 0xaa4444,
	};

	const hoverColors = {
		XY: 0x6666dd,
		XZ: 0x66dd66,
		YZ: 0xdd6666,
	};

	const selectedColors = {
		XY: 0x8888ff,
		XZ: 0x88ff88,
		YZ: 0xff8888,
	};

	// --- Datum Planes (XY, XZ, YZ) ---

	const planeSize = 1.5;
	const planeGeometry = new THREE.PlaneGeometry(planeSize * 2, planeSize * 2);

	// Create materials that we'll update reactively
	const xyPlaneMaterial = new THREE.MeshBasicMaterial({
		color: baseColors.XY,
		transparent: true,
		opacity: 0.02,
		side: THREE.DoubleSide,
		depthWrite: false
	});

	const xzPlaneMaterial = new THREE.MeshBasicMaterial({
		color: baseColors.XZ,
		transparent: true,
		opacity: 0.02,
		side: THREE.DoubleSide,
		depthWrite: false
	});

	const yzPlaneMaterial = new THREE.MeshBasicMaterial({
		color: baseColors.YZ,
		transparent: true,
		opacity: 0.02,
		side: THREE.DoubleSide,
		depthWrite: false
	});

	/**
	 * Get opacity and color for a datum plane based on hover/selection state.
	 * @param {'XY' | 'XZ' | 'YZ'} plane
	 */
	function getPlaneStyle(plane) {
		const ref = datumRefs[plane];
		const selected = getSelectedRefs().some((r) => geomRefEquals(r, ref));
		const hovered = geomRefEquals(getHoveredRef(), ref);

		if (selected) return { opacity: 0.25, color: selectedColors[plane] };
		if (hovered) return { opacity: 0.15, color: hoverColors[plane] };
		return { opacity: 0.02, color: baseColors[plane] };
	}

	// Reactive material updates
	let xyStyle = $derived(getPlaneStyle('XY'));
	let xzStyle = $derived(getPlaneStyle('XZ'));
	let yzStyle = $derived(getPlaneStyle('YZ'));

	$effect(() => {
		xyPlaneMaterial.opacity = xyStyle.opacity;
		xyPlaneMaterial.color.setHex(xyStyle.color);
	});

	$effect(() => {
		xzPlaneMaterial.opacity = xzStyle.opacity;
		xzPlaneMaterial.color.setHex(xzStyle.color);
	});

	$effect(() => {
		yzPlaneMaterial.opacity = yzStyle.opacity;
		yzPlaneMaterial.color.setHex(yzStyle.color);
	});

	// Event handlers
	function handleClick(plane, event) {
		const additive = event.nativeEvent?.shiftKey ?? false;
		selectRef(datumRefs[plane], additive);
	}

	function handlePointerEnter(plane) {
		setHoveredRef(datumRefs[plane]);
	}

	function handlePointerLeave(plane) {
		// Only clear if we're still hovering this plane
		if (geomRefEquals(getHoveredRef(), datumRefs[plane])) {
			setHoveredRef(null);
		}
	}

	// Border geometries for each plane
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

	const borderGeo = buildPlaneBorder(planeSize);

	const xyBorderMaterial = new THREE.LineBasicMaterial({ color: 0x6666cc, transparent: true, opacity: 0.08 });
	const xzBorderMaterial = new THREE.LineBasicMaterial({ color: 0x66cc66, transparent: true, opacity: 0.08 });
	const yzBorderMaterial = new THREE.LineBasicMaterial({ color: 0xcc6666, transparent: true, opacity: 0.08 });

	// XZ plane needs 90deg rotation around X
	const xzRotation = [-Math.PI / 2, 0, 0];
	// YZ plane needs 90deg rotation around Y
	const yzRotation = [0, Math.PI / 2, 0];

	// --- Origin Triad ---

	const axisLength = 3;

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

	// Arrowhead cones
	const coneGeo = new THREE.ConeGeometry(0.08, 0.3, 8);

	const xConeMaterial = new THREE.MeshBasicMaterial({ color: 0xff4444 });
	const yConeMaterial = new THREE.MeshBasicMaterial({ color: 0x44cc44 });
	const zConeMaterial = new THREE.MeshBasicMaterial({ color: 0x4488ff });

	// Cone rotations to point along each axis
	const xConeRotation = [0, 0, -Math.PI / 2];
	const yConeRotation = [0, 0, 0];
	const zConeRotation = [Math.PI / 2, 0, 0];

	// Origin sphere
	const originGeo = new THREE.SphereGeometry(0.06, 12, 8);
	const originMaterial = new THREE.MeshBasicMaterial({ color: 0xcccccc });
</script>

<!-- XY Plane (blue, Z=0) -->
<T.Group>
	<T.Mesh
		geometry={planeGeometry}
		material={xyPlaneMaterial}
		onclick={(e) => handleClick('XY', e)}
		onpointerenter={() => handlePointerEnter('XY')}
		onpointerleave={() => handlePointerLeave('XY')}
	/>
	<T.LineSegments geometry={borderGeo} material={xyBorderMaterial} />
</T.Group>

<!-- XZ Plane (green, Y=0) -->
<T.Group rotation={xzRotation}>
	<T.Mesh
		geometry={planeGeometry}
		material={xzPlaneMaterial}
		onclick={(e) => handleClick('XZ', e)}
		onpointerenter={() => handlePointerEnter('XZ')}
		onpointerleave={() => handlePointerLeave('XZ')}
	/>
	<T.LineSegments geometry={borderGeo} material={xzBorderMaterial} />
</T.Group>

<!-- YZ Plane (red, X=0) -->
<T.Group rotation={yzRotation}>
	<T.Mesh
		geometry={planeGeometry}
		material={yzPlaneMaterial}
		onclick={(e) => handleClick('YZ', e)}
		onpointerenter={() => handlePointerEnter('YZ')}
		onpointerleave={() => handlePointerLeave('YZ')}
	/>
	<T.LineSegments geometry={borderGeo} material={yzBorderMaterial} />
</T.Group>

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
