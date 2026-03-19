<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getExtrudePreviewParams, getRevolvePreviewParams, getFeatureTree, getMeshes } from '$lib/engine/store.svelte.js';
	import { buildSketchPlane } from '$lib/sketch/sketchCoords.js';
	import { extractFaceBoundary, findFaceRangeByRef } from '$lib/viewport/faceGeometry.js';
	import { sampleBSpline } from '$lib/sketch/bspline.js';

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

			// Build a lookup of spline segments by start_point_index
			const splineMap = new Map();
			if (profile.spline_segments) {
				for (const seg of profile.spline_segments) {
					splineMap.set(seg.start_point_index, seg);
				}
			}

			const points2d = [];
			const n = profile.entity_ids.length;
			for (let i = 0; i < n; i++) {
				const ptId = profile.entity_ids[i];
				const pos = positions[ptId];
				if (!pos) continue;

				// Check if this edge (i → i+1) has a spline segment
				const seg = splineMap.get(i);
				if (seg && seg.control_points?.length >= 2) {
					// Sample the B-spline and add intermediate points
					const ctrlPts = seg.control_points.map(cp => ({ x: cp[0], y: cp[1] }));
					const sampled = sampleBSpline(ctrlPts, 16);
					// Add all but the last sample (next entity's start handles it)
					for (let s = 0; s < sampled.length - 1; s++) {
						points2d.push(new THREE.Vector2(sampled[s].x, sampled[s].y));
					}
				} else {
					points2d.push(new THREE.Vector2(pos[0], pos[1]));
				}
			}
			if (points2d.length < 3) return null;
			shape = new THREE.Shape(points2d);
		}
		const effectiveDepth = Math.max(params.depth, 0.00001);
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

			// Build spline segment lookup
			const splineMap = new Map();
			if (profile.spline_segments) {
				for (const seg of profile.spline_segments) {
					splineMap.set(seg.start_point_index, seg);
				}
			}

			const n = profile.entity_ids.length;
			for (let i = 0; i < n; i++) {
				const ptId = profile.entity_ids[i];
				const pos = positions[ptId];
				if (!pos) continue;

				const seg = splineMap.get(i);
				if (seg && seg.control_points?.length >= 2) {
					const ctrlPts = seg.control_points.map(cp => ({ x: cp[0], y: cp[1] }));
					const sampled = sampleBSpline(ctrlPts, 16);
					for (let s = 0; s < sampled.length - 1; s++) {
						const pt = sp.origin.clone()
							.addScaledVector(sp.xAxis, sampled[s].x)
							.addScaledVector(sp.yAxis, sampled[s].y);
						points3d.push(pt);
					}
				} else {
					const pt = sp.origin.clone()
						.addScaledVector(sp.xAxis, pos[0])
						.addScaledVector(sp.yAxis, pos[1]);
					points3d.push(pt);
				}
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
			// LatheGeometry at phi=0 places the profile along +Z in local space
			// (sin(0)=0, cos(0)=1 → vertex.z = radius). We need the +Z direction
			// (after yUp→axisDir rotation) to align with firstRadial.
			const localZ = new THREE.Vector3(0, 0, 1).applyQuaternion(quaternion);
			const correctionQuat = new THREE.Quaternion().setFromUnitVectors(localZ, firstRadial);
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

	function buildFacePreview(params) {
		const meshData = getMeshes();
		const faceData = findFaceRangeByRef(meshData, params.geomRef);
		if (!faceData) return null;

		const boundary = extractFaceBoundary(faceData.mesh, faceData.range);
		if (boundary.length < 3) return null;

		// Compute face plane from boundary (centroid + cross product normal)
		let cx = 0, cy = 0, cz = 0;
		for (const [x, y, z] of boundary) { cx += x; cy += y; cz += z; }
		cx /= boundary.length; cy /= boundary.length; cz /= boundary.length;

		const v0 = new THREE.Vector3(boundary[1][0] - boundary[0][0], boundary[1][1] - boundary[0][1], boundary[1][2] - boundary[0][2]);
		const v1 = new THREE.Vector3(boundary[2][0] - boundary[0][0], boundary[2][1] - boundary[0][1], boundary[2][2] - boundary[0][2]);
		const normal = new THREE.Vector3().crossVectors(v0, v1).normalize();
		if (normal.lengthSq() < 1e-10) return null;

		const sp = buildSketchPlane([cx, cy, cz], [normal.x, normal.y, normal.z]);

		// Project boundary to 2D on the face plane
		const origin = sp.origin;
		const points2d = boundary.map(([x, y, z]) => {
			const rel = new THREE.Vector3(x - origin.x, y - origin.y, z - origin.z);
			return new THREE.Vector2(rel.dot(sp.xAxis), rel.dot(sp.yAxis));
		});

		const shape = new THREE.Shape(points2d);
		const effectiveDepth = Math.max(params.depth, 0.00001);
		const extrudeDepth = params.symmetric ? effectiveDepth * 2 : effectiveDepth;

		const geometry = new THREE.ExtrudeGeometry(shape, {
			depth: extrudeDepth,
			bevelEnabled: false
		});
		const edgeGeometry = new THREE.EdgesGeometry(geometry);

		const flipVisual = params.flipDirection !== params.cut;
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

	// Use a simple reactive variable: when params change, rebuild.
	// Avoid $effect to prevent write-read loops.
	let currentPreviews = $derived.by(() => {
		const raw = getExtrudePreviewParams();
		if (!raw) return [];
		const arr = Array.isArray(raw) ? raw : [raw];
		return arr.map(p => {
			if (p.type === 'face') return buildFacePreview(p);
			return buildPreview(p);
		}).filter(Boolean);
	});

	let currentRevolvePreview = $derived.by(() => {
		const params = getRevolvePreviewParams();
		if (!params) return null;
		return buildRevolvePreview(params);
	});
</script>

{#each currentPreviews as preview}
	<T.Mesh
		geometry={preview.geometry}
		material={preview.material}
		position={preview.position}
		rotation={[preview.rotation.x, preview.rotation.y, preview.rotation.z]}
		renderOrder={999}
	/>
	<T.LineSegments
		geometry={preview.edgeGeometry}
		material={edgeMaterial}
		position={preview.position}
		rotation={[preview.rotation.x, preview.rotation.y, preview.rotation.z]}
		renderOrder={999}
	/>
{/each}

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
