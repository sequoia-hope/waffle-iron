<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getExtrudePreviewParams, getRevolvePreviewParams, getFeatureTree } from '$lib/engine/store.svelte.js';
	import { buildSketchPlane } from '$lib/sketch/sketchCoords.js';

	const bossMaterial = new THREE.MeshStandardMaterial({
		color: 0x4499ff,
		opacity: 0.25,
		transparent: true,
		depthWrite: false,
		side: THREE.DoubleSide
	});

	const cutMaterial = new THREE.MeshStandardMaterial({
		color: 0xff6644,
		opacity: 0.25,
		transparent: true,
		depthWrite: false,
		side: THREE.DoubleSide
	});

	const revolveMaterial = new THREE.MeshStandardMaterial({
		color: 0x44ff99,
		opacity: 0.25,
		transparent: true,
		depthWrite: false,
		side: THREE.DoubleSide
	});

	const edgeMaterial = new THREE.LineBasicMaterial({
		color: 0x66bbff,
		opacity: 0.5,
		transparent: true
	});

	const revolveEdgeMaterial = new THREE.LineBasicMaterial({
		color: 0x66ffbb,
		opacity: 0.5,
		transparent: true
	});

	function buildPreview(params) {
		const tree = getFeatureTree();
		if (!tree || !tree.features) return null;

		const sketch = tree.features.find(f => f.id === params.sketchId);
		if (!sketch?.operation?.sketch) return null;

		const sketchData = sketch.operation.sketch;
		const profiles = sketchData.solved_profiles;
		const positions = sketchData.solved_positions;
		if (!profiles || !positions) return null;

		const profile = profiles[params.profileIndex];
		if (!profile) return null;

		let shape;
		if (profile.circle) {
			const c = profile.circle;
			shape = new THREE.Shape();
			shape.absarc(c.center_u, c.center_v, c.radius, 0, Math.PI * 2, false);
		} else {
			if (!profile.entity_ids || profile.entity_ids.length < 3) return null;
			const points2d = [];
			for (const ptId of profile.entity_ids) {
				const pos = positions[ptId];
				if (!pos) continue;
				points2d.push(new THREE.Vector2(pos[0], pos[1]));
			}
			if (points2d.length < 3) return null;
			shape = new THREE.Shape(points2d);
		}
		const effectiveDepth = Math.max(params.depth, 0.01);
		const extrudeDepth = params.symmetric ? effectiveDepth * 2 : effectiveDepth;

		const geometry = new THREE.ExtrudeGeometry(shape, {
			depth: extrudeDepth,
			bevelEnabled: false
		});
		const edgeGeometry = new THREE.EdgesGeometry(geometry);

		const planeOrigin = sketchData.plane_origin || [0, 0, 0];
		const planeNormal = sketchData.plane_normal || [0, 0, 1];
		const sp = buildSketchPlane(planeOrigin, planeNormal);

		const flipVisual = params.flipDirection !== params.cut;

		// Always use the un-flipped right-handed basis for profile orientation.
		// The engine never mirrors xAxis on flip — only the extrude direction changes.
		// Use position offset to show the flipped direction visually.
		const basis = new THREE.Matrix4().makeBasis(sp.xAxis, sp.yAxis, sp.normal);
		const quaternion = new THREE.Quaternion().setFromRotationMatrix(basis);

		const position = sp.origin.clone();
		if (params.symmetric) {
			position.addScaledVector(sp.normal, -effectiveDepth);
		} else if (flipVisual) {
			position.addScaledVector(sp.normal, -extrudeDepth);
		}

		return {
			geometry,
			edgeGeometry,
			material: params.cut ? cutMaterial : bossMaterial,
			position: [position.x, position.y, position.z],
			rotation: new THREE.Euler().setFromQuaternion(quaternion)
		};
	}

	function buildRevolvePreview(params) {
		const tree = getFeatureTree();
		if (!tree || !tree.features) return null;

		const sketch = tree.features.find(f => f.id === params.sketchId);
		if (!sketch?.operation?.sketch) return null;

		const sketchData = sketch.operation.sketch;
		const profiles = sketchData.solved_profiles;
		const positions = sketchData.solved_positions;
		if (!profiles || !positions) return null;

		const profile = profiles[params.profileIndex];
		if (!profile) return null;

		// Get sketch plane info for transforming 2D sketch coords to 3D
		const planeOrigin = sketchData.plane_origin || [0, 0, 0];
		const planeNormal = sketchData.plane_normal || [0, 0, 1];
		const sp = buildSketchPlane(planeOrigin, planeNormal);

		// Collect 3D profile points
		const points3d = [];
		if (profile.circle) {
			// Sample circle at N points for revolve preview
			const c = profile.circle;
			const N = 32;
			for (let i = 0; i < N; i++) {
				const angle = (2 * Math.PI * i) / N;
				const u = c.center_u + c.radius * Math.cos(angle);
				const v = c.center_v + c.radius * Math.sin(angle);
				const pt = sp.origin.clone()
					.addScaledVector(sp.xAxis, u)
					.addScaledVector(sp.yAxis, v);
				points3d.push(pt);
			}
		} else {
			if (!profile.entity_ids || profile.entity_ids.length < 3) return null;
			for (const ptId of profile.entity_ids) {
				const pos = positions[ptId];
				if (!pos) continue;
				const pt = sp.origin.clone()
					.addScaledVector(sp.xAxis, pos[0])
					.addScaledVector(sp.yAxis, pos[1]);
				points3d.push(pt);
			}
		}
		if (points3d.length < 3) return null;

		// Revolution axis in world space
		const axisOrigin = new THREE.Vector3(params.axisOrigin[0], params.axisOrigin[1], params.axisOrigin[2]);
		const axisDir = new THREE.Vector3(params.axisDir[0], params.axisDir[1], params.axisDir[2]);
		if (axisDir.lengthSq() < 1e-10) return null;
		axisDir.normalize();

		// Build a local coordinate system for the lathe:
		// LatheGeometry revolves around +Y axis. We need to map:
		//   axisDir -> +Y (lathe axis)
		//   radial direction -> +X (lathe radius)
		// Points for LatheGeometry are Vector2(radius, height) where
		// radius = distance from axis, height = position along axis.

		// Project profile points onto the axis plane and compute (radius, height) pairs
		const lathePoints = [];
		for (const pt of points3d) {
			const toPoint = pt.clone().sub(axisOrigin);
			const height = toPoint.dot(axisDir);
			const radialVec = toPoint.clone().addScaledVector(axisDir, -height);
			const radius = radialVec.length();
			// LatheGeometry expects Vector2(x, y) where x=radius, y=height
			lathePoints.push(new THREE.Vector2(radius, height));
		}

		// Clamp angle
		const angleDeg = Math.max(0.1, Math.min(360, params.angle));
		const angleRad = angleDeg * Math.PI / 180;
		const segments = Math.max(8, Math.round(angleDeg / 5));

		const geometry = new THREE.LatheGeometry(lathePoints, segments, 0, angleRad);
		const edgeGeometry = new THREE.EdgesGeometry(geometry);

		// Build rotation matrix to orient the lathe from Y-up to axisDir
		// LatheGeometry revolves around Y, so we need to rotate Y -> axisDir
		const yUp = new THREE.Vector3(0, 1, 0);
		const quaternion = new THREE.Quaternion().setFromUnitVectors(yUp, axisDir);

		// We also need to rotate around the axis so the profile starts at the right radial position.
		// Find the radial direction of the first profile point to set the starting angle.
		const firstPt = points3d[0].clone().sub(axisOrigin);
		const firstHeight = firstPt.dot(axisDir);
		const firstRadial = firstPt.clone().addScaledVector(axisDir, -firstHeight);
		if (firstRadial.lengthSq() > 1e-10) {
			firstRadial.normalize();
			// The lathe starts at +X in local space. We need the +X direction (after yUp->axisDir rotation)
			// to align with firstRadial.
			const localX = new THREE.Vector3(1, 0, 0).applyQuaternion(quaternion);
			const correctionQuat = new THREE.Quaternion().setFromUnitVectors(localX, firstRadial);
			quaternion.premultiply(correctionQuat);
		}

		const euler = new THREE.Euler().setFromQuaternion(quaternion);

		return {
			geometry,
			edgeGeometry,
			material: revolveMaterial,
			position: [axisOrigin.x, axisOrigin.y, axisOrigin.z],
			rotation: euler
		};
	}

	// Use a simple reactive variable: when params change, rebuild.
	// Avoid $effect to prevent write-read loops.
	let currentPreview = $derived.by(() => {
		const params = getExtrudePreviewParams();
		if (!params) return null;
		return buildPreview(params);
	});

	let currentRevolvePreview = $derived.by(() => {
		const params = getRevolvePreviewParams();
		if (!params) return null;
		return buildRevolvePreview(params);
	});
</script>

{#if currentPreview}
	<T.Mesh
		geometry={currentPreview.geometry}
		material={currentPreview.material}
		position={currentPreview.position}
		rotation={[currentPreview.rotation.x, currentPreview.rotation.y, currentPreview.rotation.z]}
		renderOrder={999}
	/>
	<T.LineSegments
		geometry={currentPreview.edgeGeometry}
		material={edgeMaterial}
		position={currentPreview.position}
		rotation={[currentPreview.rotation.x, currentPreview.rotation.y, currentPreview.rotation.z]}
		renderOrder={999}
	/>
{/if}

{#if currentRevolvePreview}
	<T.Mesh
		geometry={currentRevolvePreview.geometry}
		material={currentRevolvePreview.material}
		position={currentRevolvePreview.position}
		rotation={[currentRevolvePreview.rotation.x, currentRevolvePreview.rotation.y, currentRevolvePreview.rotation.z]}
		renderOrder={999}
	/>
	<T.LineSegments
		geometry={currentRevolvePreview.edgeGeometry}
		material={revolveEdgeMaterial}
		position={currentRevolvePreview.position}
		rotation={[currentRevolvePreview.rotation.x, currentRevolvePreview.rotation.y, currentRevolvePreview.rotation.z]}
		renderOrder={999}
	/>
{/if}
