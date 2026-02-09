<script>
	import { T, useThrelte } from '@threlte/core';
	import * as THREE from 'three';
	import {
		getSketchMode,
		getSketchEntities,
		getSketchPositions,
		getSketchSelection,
		getSketchHover,
		getSketchConstraints
	} from '$lib/engine/store.svelte.js';
	import { getPreview, getSnapIndicator } from './tools.js';
	import { buildSketchPlane, sketchToWorld } from './sketchCoords.js';

	// Color scheme
	const COLOR_DEFAULT = 0x4488ff;    // blue, under-constrained
	const COLOR_SELECTED = 0xffdd44;   // yellow, selected
	const COLOR_HOVERED = 0x88bbff;    // light blue, hovered
	const COLOR_PREVIEW = 0x6699cc;    // dimmer blue, preview
	const COLOR_SNAP = 0x44cc44;       // green, snap indicator

	let sm = $derived(getSketchMode());
	let entities = $derived(getSketchEntities());
	let positions = $derived(getSketchPositions());
	let selection = $derived(getSketchSelection());
	let hoverEntity = $derived(getSketchHover());
	let constraints = $derived(getSketchConstraints());

	let plane = $derived(sm?.active ? buildSketchPlane(sm.origin, sm.normal) : null);

	/**
	 * Get entity color based on selection/hover state.
	 * @param {number} entityId
	 * @returns {number}
	 */
	function entityColor(entityId) {
		if (selection.has(entityId)) return COLOR_SELECTED;
		if (hoverEntity === entityId) return COLOR_HOVERED;
		return COLOR_DEFAULT;
	}

	// -- Build geometry for entities --

	/**
	 * Build point sphere positions in sketch-local coordinates.
	 */
	let pointData = $derived.by(() => {
		if (!plane) return [];
		return entities
			.filter(e => e.type === 'Point')
			.map(e => {
				const pos = positions.get(e.id);
				if (!pos) return null;
				return { id: e.id, world: sketchToWorld(pos.x, pos.y, plane) };
			})
			.filter(Boolean);
	});

	/**
	 * Build line segment data.
	 */
	let lineData = $derived.by(() => {
		if (!plane) return [];
		return entities
			.filter(e => e.type === 'Line')
			.map(e => {
				const p1 = positions.get(e.start_id);
				const p2 = positions.get(e.end_id);
				if (!p1 || !p2) return null;
				const w1 = sketchToWorld(p1.x, p1.y, plane);
				const w2 = sketchToWorld(p2.x, p2.y, plane);
				const geo = new THREE.BufferGeometry().setFromPoints([w1, w2]);
				return { id: e.id, geometry: geo };
			})
			.filter(Boolean);
	});

	/**
	 * Build circle geometry (64-segment loop).
	 */
	let circleData = $derived.by(() => {
		if (!plane) return [];
		return entities
			.filter(e => e.type === 'Circle')
			.map(e => {
				const center = positions.get(e.center_id);
				if (!center) return null;
				const segments = 64;
				const points = [];
				for (let i = 0; i <= segments; i++) {
					const angle = (i / segments) * Math.PI * 2;
					const x = center.x + Math.cos(angle) * e.radius;
					const y = center.y + Math.sin(angle) * e.radius;
					points.push(sketchToWorld(x, y, plane));
				}
				const geo = new THREE.BufferGeometry().setFromPoints(points);
				return { id: e.id, geometry: geo };
			})
			.filter(Boolean);
	});

	/**
	 * Build arc geometry.
	 */
	let arcData = $derived.by(() => {
		if (!plane) return [];
		return entities
			.filter(e => e.type === 'Arc')
			.map(e => {
				const center = positions.get(e.center_id);
				const startPt = positions.get(e.start_id);
				const endPt = positions.get(e.end_id);
				if (!center || !startPt || !endPt) return null;

				const dx = startPt.x - center.x;
				const dy = startPt.y - center.y;
				const radius = Math.sqrt(dx * dx + dy * dy);
				let startAngle = Math.atan2(startPt.y - center.y, startPt.x - center.x);
				let endAngle = Math.atan2(endPt.y - center.y, endPt.x - center.x);

				// Ensure CCW sweep
				if (endAngle <= startAngle) endAngle += Math.PI * 2;

				const segments = 48;
				const points = [];
				for (let i = 0; i <= segments; i++) {
					const t = i / segments;
					const angle = startAngle + t * (endAngle - startAngle);
					const x = center.x + Math.cos(angle) * radius;
					const y = center.y + Math.sin(angle) * radius;
					points.push(sketchToWorld(x, y, plane));
				}
				const geo = new THREE.BufferGeometry().setFromPoints(points);
				return { id: e.id, geometry: geo };
			})
			.filter(Boolean);
	});

	// -- Preview geometry --

	let previewGeo = $derived.by(() => {
		const preview = getPreview();
		if (!preview || !plane) return null;

		if (preview.type === 'line') {
			const { x1, y1, x2, y2 } = preview.data;
			const w1 = sketchToWorld(x1, y1, plane);
			const w2 = sketchToWorld(x2, y2, plane);
			return { type: 'line', geometry: new THREE.BufferGeometry().setFromPoints([w1, w2]) };
		}

		if (preview.type === 'rectangle') {
			const { x1, y1, x2, y2 } = preview.data;
			const corners = [
				sketchToWorld(x1, y1, plane),
				sketchToWorld(x2, y1, plane),
				sketchToWorld(x2, y2, plane),
				sketchToWorld(x1, y2, plane),
				sketchToWorld(x1, y1, plane)
			];
			return { type: 'line', geometry: new THREE.BufferGeometry().setFromPoints(corners) };
		}

		if (preview.type === 'circle') {
			const { cx, cy, radius } = preview.data;
			const segments = 64;
			const points = [];
			for (let i = 0; i <= segments; i++) {
				const angle = (i / segments) * Math.PI * 2;
				points.push(sketchToWorld(cx + Math.cos(angle) * radius, cy + Math.sin(angle) * radius, plane));
			}
			return { type: 'line', geometry: new THREE.BufferGeometry().setFromPoints(points) };
		}

		if (preview.type === 'arc' || preview.type === 'arc-preview-radius') {
			const d = preview.data;
			if (preview.type === 'arc-preview-radius') {
				const w1 = sketchToWorld(d.cx, d.cy, plane);
				const w2 = sketchToWorld(d.ex, d.ey, plane);
				return { type: 'line', geometry: new THREE.BufferGeometry().setFromPoints([w1, w2]) };
			}
			const { cx, cy, radius, startAngle, endAngle } = d;
			let end = endAngle;
			if (end <= startAngle) end += Math.PI * 2;
			const segments = 48;
			const points = [];
			for (let i = 0; i <= segments; i++) {
				const t = i / segments;
				const angle = startAngle + t * (end - startAngle);
				points.push(sketchToWorld(cx + Math.cos(angle) * radius, cy + Math.sin(angle) * radius, plane));
			}
			return { type: 'line', geometry: new THREE.BufferGeometry().setFromPoints(points) };
		}

		return null;
	});

	// -- Snap indicator geometry --

	let snapGeo = $derived.by(() => {
		const snap = getSnapIndicator();
		if (!snap || !plane) return null;

		if (snap.type === 'coincident') {
			return { type: 'point', world: sketchToWorld(snap.x, snap.y, plane) };
		}

		if (snap.type === 'horizontal' || snap.type === 'vertical') {
			const w1 = sketchToWorld(snap.fromX, snap.fromY, plane);
			const w2 = sketchToWorld(snap.x, snap.y, plane);
			return { type: 'dashed-line', geometry: new THREE.BufferGeometry().setFromPoints([w1, w2]) };
		}

		if (snap.type === 'on-entity') {
			return { type: 'point', world: sketchToWorld(snap.x, snap.y, plane) };
		}

		return null;
	});

	// Constraint label data
	let constraintLabels = $derived.by(() => {
		if (!plane) return [];
		const labels = [];
		for (const c of constraints) {
			if (c.type === 'Horizontal' || c.type === 'Vertical') {
				const entity = entities.find(e => e.id === c.entity);
				if (entity && entity.type === 'Line') {
					const p1 = positions.get(entity.start_id);
					const p2 = positions.get(entity.end_id);
					if (p1 && p2) {
						const mx = (p1.x + p2.x) / 2;
						const my = (p1.y + p2.y) / 2;
						labels.push({
							text: c.type === 'Horizontal' ? 'H' : 'V',
							world: sketchToWorld(mx, my + 0.15, plane)
						});
					}
				}
			}
		}
		return labels;
	});

	// Shared materials
	const previewMaterial = new THREE.LineBasicMaterial({ color: COLOR_PREVIEW, depthTest: false, transparent: true, opacity: 0.6 });
	const previewDashedMaterial = new THREE.LineDashedMaterial({ color: COLOR_PREVIEW, depthTest: false, transparent: true, opacity: 0.6, dashSize: 0.1, gapSize: 0.05 });
	const snapDashedMaterial = new THREE.LineDashedMaterial({ color: COLOR_SNAP, depthTest: false, transparent: true, opacity: 0.8, dashSize: 0.08, gapSize: 0.04 });
	const pointGeometry = new THREE.SphereGeometry(0.06, 8, 8);
	const snapPointGeometry = new THREE.SphereGeometry(0.08, 8, 8);
	const snapPointMaterial = new THREE.MeshBasicMaterial({ color: COLOR_SNAP, depthTest: false });
</script>

{#if sm?.active && plane}
	<!-- Entity points -->
	{#each pointData as pt (pt.id)}
		<T.Mesh geometry={pointGeometry} position={[pt.world.x, pt.world.y, pt.world.z]} renderOrder={10}>
			<T.MeshBasicMaterial color={entityColor(pt.id)} depthTest={false} />
		</T.Mesh>
	{/each}

	<!-- Entity lines -->
	{#each lineData as line (line.id)}
		<T.Line geometry={line.geometry} renderOrder={10}>
			<T.LineBasicMaterial color={entityColor(line.id)} depthTest={false} linewidth={1} />
		</T.Line>
	{/each}

	<!-- Entity circles -->
	{#each circleData as circle (circle.id)}
		<T.Line geometry={circle.geometry} renderOrder={10}>
			<T.LineBasicMaterial color={entityColor(circle.id)} depthTest={false} linewidth={1} />
		</T.Line>
	{/each}

	<!-- Entity arcs -->
	{#each arcData as arc (arc.id)}
		<T.Line geometry={arc.geometry} renderOrder={10}>
			<T.LineBasicMaterial color={entityColor(arc.id)} depthTest={false} linewidth={1} />
		</T.Line>
	{/each}

	<!-- Preview geometry -->
	{#if previewGeo}
		{#if previewGeo.type === 'line'}
			<T.Line geometry={previewGeo.geometry} material={previewDashedMaterial} renderOrder={10} />
		{/if}
	{/if}

	<!-- Snap indicators -->
	{#if snapGeo}
		{#if snapGeo.type === 'point'}
			<T.Mesh geometry={snapPointGeometry} material={snapPointMaterial}
				position={[snapGeo.world.x, snapGeo.world.y, snapGeo.world.z]} renderOrder={11} />
		{:else if snapGeo.type === 'dashed-line'}
			<T.Line geometry={snapGeo.geometry} material={snapDashedMaterial} renderOrder={11} />
		{/if}
	{/if}

	<!-- Constraint labels -->
	{#each constraintLabels as label, i}
		<T.Mesh position={[label.world.x, label.world.y, label.world.z]} renderOrder={12}>
			<T.PlaneGeometry args={[0.12, 0.12]} />
			<T.MeshBasicMaterial color={COLOR_DEFAULT} depthTest={false} transparent opacity={0.7} />
		</T.Mesh>
	{/each}
{/if}
