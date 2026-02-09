<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';

	// --- Datum Planes (XY, XZ, YZ) ---

	const planeSize = 5;
	const planeGeometry = new THREE.PlaneGeometry(planeSize * 2, planeSize * 2);

	const xyPlaneMaterial = new THREE.MeshBasicMaterial({
		color: 0x4444aa,
		transparent: true,
		opacity: 0.06,
		side: THREE.DoubleSide,
		depthWrite: false
	});

	const xzPlaneMaterial = new THREE.MeshBasicMaterial({
		color: 0x44aa44,
		transparent: true,
		opacity: 0.06,
		side: THREE.DoubleSide,
		depthWrite: false
	});

	const yzPlaneMaterial = new THREE.MeshBasicMaterial({
		color: 0xaa4444,
		transparent: true,
		opacity: 0.06,
		side: THREE.DoubleSide,
		depthWrite: false
	});

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

	const xyBorderMaterial = new THREE.LineBasicMaterial({ color: 0x6666cc, transparent: true, opacity: 0.3 });
	const xzBorderMaterial = new THREE.LineBasicMaterial({ color: 0x66cc66, transparent: true, opacity: 0.3 });
	const yzBorderMaterial = new THREE.LineBasicMaterial({ color: 0xcc6666, transparent: true, opacity: 0.3 });

	// XZ plane needs 90° rotation around X
	const xzRotation = [-Math.PI / 2, 0, 0];
	// YZ plane needs 90° rotation around Y
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
	<T.Mesh geometry={planeGeometry} material={xyPlaneMaterial} />
	<T.LineSegments geometry={borderGeo} material={xyBorderMaterial} />
</T.Group>

<!-- XZ Plane (green, Y=0) -->
<T.Group rotation={xzRotation}>
	<T.Mesh geometry={planeGeometry} material={xzPlaneMaterial} />
	<T.LineSegments geometry={borderGeo} material={xzBorderMaterial} />
</T.Group>

<!-- YZ Plane (red, X=0) -->
<T.Group rotation={yzRotation}>
	<T.Mesh geometry={planeGeometry} material={yzPlaneMaterial} />
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
