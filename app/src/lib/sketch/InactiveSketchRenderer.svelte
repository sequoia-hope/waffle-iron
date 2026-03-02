<script>
	import { T, useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import {
		getFeatureTree,
		getSketchMode,
		getSelectedFeatureId,
		isSketchVisible,
		getEditingSketchFeatureId,
		getInactiveHoveredProfile,
		setInactiveHoveredProfile,
		getCameraObject,
		getHoveredRef
	} from '$lib/engine/store.svelte.js';
	import { buildSketchPlane, sketchToWorld } from './sketchCoords.js';
	import { extractProfiles, profileToPolygon, pointInPolygon } from './profiles.js';
	import { sampleBSpline } from './bspline.js';

	const { renderer } = useThrelte();

	const COLOR_INACTIVE = 0x888888;
	const COLOR_SELECTED = 0xff8800;
	const COLOR_PROFILE_HOVER = 0x4488ff;

	let tree = $derived(getFeatureTree());
	let sm = $derived(getSketchMode());
	let selectedId = $derived(getSelectedFeatureId());
	let editingId = $derived(getEditingSketchFeatureId());

	/**
	 * Get all sketch features that should show inactive wireframes.
	 */
	let sketchFeatures = $derived.by(() => {
		if (sm?.active) return [];
		if (!tree?.features) return [];
		return tree.features.filter(f =>
			f.operation?.type === 'Sketch' &&
			!f.suppressed &&
			f.operation.sketch &&
			isSketchVisible(f.id) &&
			f.id !== editingId
		);
	});

	/**
	 * Parse sketch data into entities, positions, plane, and profiles.
	 */
	let sketchData = $derived.by(() => {
		const result = [];
		for (const feature of sketchFeatures) {
			const sketch = feature.operation.sketch;
			const origin = sketch.plane_origin || [0, 0, 0];
			const normal = sketch.plane_normal || [0, 0, 1];
			const plane = buildSketchPlane(origin, normal);

			const positions = new Map();
			const savedPos = sketch.solved_positions || {};
			for (const [id, coords] of Object.entries(savedPos)) {
				if (Array.isArray(coords) && coords.length >= 2) {
					positions.set(Number(id), { x: coords[0], y: coords[1] });
				}
			}

			const entities = sketch.entities || [];
			const profiles = extractProfiles(entities, positions);

			result.push({ featureId: feature.id, sketch, plane, positions, entities, profiles });
		}
		return result;
	});

	/**
	 * Build wireframe geometry for each inactive sketch.
	 */
	let sketchWireframes = $derived.by(() => {
		const result = [];
		for (const data of sketchData) {
			const { featureId, plane, positions, entities } = data;
			const isSelected = selectedId === featureId;
			const color = isSelected ? COLOR_SELECTED : COLOR_INACTIVE;
			const geometries = [];

			for (const entity of entities) {
				if (entity.construction) continue;

				if (entity.type === 'Line') {
					const p1 = positions.get(entity.start_id);
					const p2 = positions.get(entity.end_id);
					if (!p1 || !p2) continue;
					const w1 = sketchToWorld(p1.x, p1.y, plane);
					const w2 = sketchToWorld(p2.x, p2.y, plane);
					geometries.push({
						type: 'line',
						geometry: new THREE.BufferGeometry().setFromPoints([w1, w2])
					});
				} else if (entity.type === 'Circle') {
					const center = positions.get(entity.center_id);
					if (!center) continue;
					const segments = 48;
					const points = [];
					for (let i = 0; i <= segments; i++) {
						const angle = (i / segments) * Math.PI * 2;
						const x = center.x + Math.cos(angle) * entity.radius;
						const y = center.y + Math.sin(angle) * entity.radius;
						points.push(sketchToWorld(x, y, plane));
					}
					geometries.push({
						type: 'line',
						geometry: new THREE.BufferGeometry().setFromPoints(points)
					});
				} else if (entity.type === 'Arc') {
					const center = positions.get(entity.center_id);
					const startPt = positions.get(entity.start_id);
					const endPt = positions.get(entity.end_id);
					if (!center || !startPt || !endPt) continue;
					const dx = startPt.x - center.x;
					const dy = startPt.y - center.y;
					const radius = Math.sqrt(dx * dx + dy * dy);
					let startAngle = Math.atan2(startPt.y - center.y, startPt.x - center.x);
					let endAngle = Math.atan2(endPt.y - center.y, endPt.x - center.x);
					if (endAngle <= startAngle) endAngle += Math.PI * 2;
					const segments = 32;
					const points = [];
					for (let i = 0; i <= segments; i++) {
						const t = i / segments;
						const angle = startAngle + t * (endAngle - startAngle);
						const x = center.x + Math.cos(angle) * radius;
						const y = center.y + Math.sin(angle) * radius;
						points.push(sketchToWorld(x, y, plane));
					}
					geometries.push({
						type: 'line',
						geometry: new THREE.BufferGeometry().setFromPoints(points)
					});
				} else if (entity.type === 'Spline' && entity.point_ids?.length >= 2) {
					const ctrlPts = entity.point_ids
						.map(pid => positions.get(pid))
						.filter(Boolean);
					if (ctrlPts.length >= 2) {
						const sampled = sampleBSpline(ctrlPts, 48);
						const worldPts = sampled.map(p => sketchToWorld(p.x, p.y, plane));
						geometries.push({
							type: 'line',
							geometry: new THREE.BufferGeometry().setFromPoints(worldPts)
						});
					}
				}
			}

			if (geometries.length > 0) {
				result.push({ featureId, geometries, color });
			}
		}
		return result;
	});

	/**
	 * Build profile fill geometry for hovered inactive sketch profile.
	 */
	let profileFills = $derived.by(() => {
		const hovered = getInactiveHoveredProfile();
		if (!hovered) return [];
		const fills = [];

		for (const data of sketchData) {
			if (data.featureId !== hovered.featureId) continue;
			const profile = data.profiles[hovered.profileIndex];
			if (!profile) continue;

			const poly = profileToPolygon(profile, data.entities, data.positions);
			if (poly.length < 3) continue;

			const shape = new THREE.Shape();
			shape.moveTo(poly[0].x, poly[0].y);
			for (let j = 1; j < poly.length; j++) {
				shape.lineTo(poly[j].x, poly[j].y);
			}
			shape.closePath();

			const shapeGeo = new THREE.ShapeGeometry(shape);
			const posAttr = shapeGeo.getAttribute('position');
			for (let v = 0; v < posAttr.count; v++) {
				const sx = posAttr.getX(v);
				const sy = posAttr.getY(v);
				const w = sketchToWorld(sx, sy, data.plane);
				posAttr.setXYZ(v, w.x, w.y, w.z);
			}
			posAttr.needsUpdate = true;

			fills.push({ key: `${data.featureId}-${hovered.profileIndex}`, geometry: shapeGeo });
		}
		return fills;
	});

	// Reusable objects for raycasting to sketch plane
	const _raycaster = new THREE.Raycaster();
	const _mouse = new THREE.Vector2();
	const _planeObj = new THREE.Plane();
	const _intersection = new THREE.Vector3();

	/**
	 * Hit-test cursor against inactive sketch profiles.
	 * @param {MouseEvent} e
	 */
	function handlePointerMove(e) {
		if (sm?.active) {
			if (getInactiveHoveredProfile()) setInactiveHoveredProfile(null);
			return;
		}

		// Don't hover profiles when a face or edge is under the cursor
		if (getHoveredRef()) {
			if (getInactiveHoveredProfile()) setInactiveHoveredProfile(null);
			return;
		}

		const camera = getCameraObject();
		if (!camera || !renderer) {
			if (getInactiveHoveredProfile()) setInactiveHoveredProfile(null);
			return;
		}

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		_mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
		_mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
		_raycaster.setFromCamera(_mouse, camera);

		// Check each sketch's profiles
		for (const data of sketchData) {
			if (data.profiles.length === 0) continue;

			const origin = data.sketch.plane_origin || [0, 0, 0];
			const normal = data.sketch.plane_normal || [0, 0, 1];
			const n = new THREE.Vector3(normal[0], normal[1], normal[2]).normalize();
			const o = new THREE.Vector3(origin[0], origin[1], origin[2]);
			_planeObj.setFromNormalAndCoplanarPoint(n, o);

			if (!_raycaster.ray.intersectPlane(_planeObj, _intersection)) continue;

			// Transform world intersection to sketch 2D
			const rel = _intersection.clone().sub(o);
			const sx = rel.dot(data.plane.xAxis);
			const sy = rel.dot(data.plane.yAxis);

			for (let i = 0; i < data.profiles.length; i++) {
				const poly = profileToPolygon(data.profiles[i], data.entities, data.positions);
				if (poly.length < 3) continue;
				if (pointInPolygon(sx, sy, poly)) {
					setInactiveHoveredProfile({ featureId: data.featureId, profileIndex: i });
					return;
				}
			}
		}

		if (getInactiveHoveredProfile()) setInactiveHoveredProfile(null);
	}

	onMount(() => {
		const canvas = renderer?.domElement;
		if (!canvas) return;
		canvas.addEventListener('pointermove', handlePointerMove);
		return () => {
			canvas.removeEventListener('pointermove', handlePointerMove);
		};
	});
</script>

{#each sketchWireframes as wireframe (wireframe.featureId)}
	{#each wireframe.geometries as geo, i}
		<T.Line geometry={geo.geometry} renderOrder={3}>
			<T.LineBasicMaterial
				color={wireframe.color}
				depthTest={true}
				transparent
				opacity={0.6}
			/>
		</T.Line>
	{/each}
{/each}

{#each profileFills as fill (fill.key)}
	<T.Mesh geometry={fill.geometry} renderOrder={2}>
		<T.MeshBasicMaterial
			color={COLOR_PROFILE_HOVER}
			transparent
			opacity={0.12}
			side={THREE.DoubleSide}
			depthTest={true}
		/>
	</T.Mesh>
{/each}
