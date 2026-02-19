<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getExtrudePreviewParams, getFeatureTree } from '$lib/engine/store.svelte.js';
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

	const edgeMaterial = new THREE.LineBasicMaterial({
		color: 0x66bbff,
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
		if (!profile || !profile.entity_ids || profile.entity_ids.length < 3) return null;

		const points2d = [];
		for (const ptId of profile.entity_ids) {
			const pos = positions[ptId];
			if (!pos) continue;
			points2d.push(new THREE.Vector2(pos[0], pos[1]));
		}
		if (points2d.length < 3) return null;

		const shape = new THREE.Shape(points2d);
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

		// XOR: show the effective visual direction of the extrude tool.
		// Boss: extends in extrude direction. Cut: extends in reversed direction (into body).
		// Flip inverts either one. XOR combines flip + cut correctly:
		//   Default boss: +normal | Flipped boss: -normal
		//   Default cut: -normal  | Flipped cut: +normal
		const flipVisual = params.flipDirection !== params.cut;
		let extrudeNormal = sp.normal.clone();
		if (flipVisual) extrudeNormal.negate();

		// Negate xAxis when normal is negated to keep basis right-handed (det=+1).
		// Left-handed basis makes setFromRotationMatrix produce wrong quaternion.
		const basisX = flipVisual ? sp.xAxis.clone().negate() : sp.xAxis;
		const basis = new THREE.Matrix4().makeBasis(basisX, sp.yAxis, extrudeNormal);
		const quaternion = new THREE.Quaternion().setFromRotationMatrix(basis);

		const position = sp.origin.clone();
		if (params.symmetric) {
			position.addScaledVector(extrudeNormal, -effectiveDepth);
		}

		return {
			geometry,
			edgeGeometry,
			material: params.cut ? cutMaterial : bossMaterial,
			position: [position.x, position.y, position.z],
			rotation: new THREE.Euler().setFromQuaternion(quaternion)
		};
	}

	// Use a simple reactive variable: when params change, rebuild.
	// Avoid $effect to prevent write-read loops.
	let currentPreview = $derived.by(() => {
		const params = getExtrudePreviewParams();
		if (!params) return null;
		return buildPreview(params);
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
