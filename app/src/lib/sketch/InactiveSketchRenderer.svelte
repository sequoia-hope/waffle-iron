<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import {
		getFeatureTree,
		getSketchMode,
		getSelectedFeatureId,
		isSketchVisible,
		getEditingSketchFeatureId
	} from '$lib/engine/store.svelte.js';
	import { buildSketchPlane, sketchToWorld } from './sketchCoords.js';

	const COLOR_INACTIVE = 0x888888;
	const COLOR_SELECTED = 0xff8800;

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
	 * Build wireframe geometry for each inactive sketch.
	 */
	let sketchWireframes = $derived.by(() => {
		const result = [];
		for (const feature of sketchFeatures) {
			const sketch = feature.operation.sketch;
			const origin = sketch.plane_origin || [0, 0, 0];
			const normal = sketch.plane_normal || [0, 0, 1];
			const plane = buildSketchPlane(origin, normal);

			// Parse positions: { "id": [x, y] }
			const positions = new Map();
			const savedPos = sketch.solved_positions || {};
			for (const [id, coords] of Object.entries(savedPos)) {
				if (Array.isArray(coords) && coords.length >= 2) {
					positions.set(Number(id), { x: coords[0], y: coords[1] });
				}
			}

			const entities = sketch.entities || [];
			const isSelected = selectedId === feature.id;
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
				}
			}

			if (geometries.length > 0) {
				result.push({ featureId: feature.id, geometries, color });
			}
		}
		return result;
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
