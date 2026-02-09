<script>
	import { T, useThrelte } from '@threlte/core';
	import * as THREE from 'three';
	import {
		getSketchMode,
		getSketchEntities,
		getSketchPositions,
		getSketchSelection,
		getSketchHover,
		getSketchConstraints,
		getExtractedProfiles,
		getSelectedProfileIndex,
		getHoveredProfileIndex,
		getOverConstrainedEntities
	} from '$lib/engine/store.svelte.js';
	import { getPreview, getSnapIndicator } from './tools.js';
	import { buildSketchPlane, sketchToWorld } from './sketchCoords.js';
	import { profileToPolygon } from './profiles.js';

	// Color scheme
	const COLOR_AXIS_X = 0xcc4444;     // red, sketch X axis
	const COLOR_AXIS_Y = 0x44aa44;     // green, sketch Y axis
	const COLOR_ORIGIN = 0xffffff;     // white, origin marker
	const COLOR_DEFAULT = 0x4488ff;    // blue, under-constrained
	const COLOR_SELECTED = 0xffdd44;   // yellow, selected
	const COLOR_HOVERED = 0x88bbff;    // light blue, hovered
	const COLOR_PREVIEW = 0x6699cc;    // dimmer blue, preview
	const COLOR_SNAP = 0x44cc44;       // green, snap indicator
	const COLOR_CONSTRUCTION = 0x6677aa; // dimmer blue, construction
	const COLOR_PROFILE_HOVER = 0x55cc88;  // green-ish, profile hover
	const COLOR_PROFILE_SELECT = 0x44ff88; // bright green, profile selected
	const COLOR_OVERCONSTRAINED = 0xff4444; // red, over-constrained

	let sm = $derived(getSketchMode());
	let entities = $derived(getSketchEntities());
	let positions = $derived(getSketchPositions());
	let selection = $derived(getSketchSelection());
	let hoverEntity = $derived(getSketchHover());
	let constraints = $derived(getSketchConstraints());
	let profiles = $derived(getExtractedProfiles());
	let selectedProfile = $derived(getSelectedProfileIndex());
	let hoveredProfile = $derived(getHoveredProfileIndex());
	let overConstrained = $derived(getOverConstrainedEntities());

	let plane = $derived(sm?.active ? buildSketchPlane(sm.origin, sm.normal) : null);

	// Build sets of entity IDs in hovered/selected profiles for fast lookup
	let hoveredProfileEntityIds = $derived.by(() => {
		if (hoveredProfile == null || hoveredProfile >= profiles.length) return new Set();
		return new Set(profiles[hoveredProfile].entityIds);
	});
	let selectedProfileEntityIds = $derived.by(() => {
		if (selectedProfile == null || selectedProfile >= profiles.length) return new Set();
		return new Set(profiles[selectedProfile].entityIds);
	});

	/**
	 * Check if entity is construction geometry.
	 * @param {number} entityId
	 * @returns {boolean}
	 */
	function isConstruction(entityId) {
		const entity = entities.find(e => e.id === entityId);
		return entity?.construction ?? false;
	}

	/**
	 * Get entity color based on selection/hover/profile state.
	 * @param {number} entityId
	 * @returns {number}
	 */
	function entityColor(entityId) {
		if (selection.has(entityId)) return COLOR_SELECTED;
		if (hoverEntity === entityId) return COLOR_HOVERED;
		if (overConstrained.has(entityId)) return COLOR_OVERCONSTRAINED;
		if (selectedProfileEntityIds.has(entityId)) return COLOR_PROFILE_SELECT;
		if (hoveredProfileEntityIds.has(entityId)) return COLOR_PROFILE_HOVER;
		if (isConstruction(entityId)) return COLOR_CONSTRUCTION;
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
				return { id: e.id, world: sketchToWorld(pos.x, pos.y, plane), construction: e.construction };
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
				return { id: e.id, geometry: geo, construction: e.construction };
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
				return { id: e.id, geometry: geo, construction: e.construction };
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
				return { id: e.id, geometry: geo, construction: e.construction };
			})
			.filter(Boolean);
	});

	// -- Profile fill geometry --

	let profileFills = $derived.by(() => {
		if (!plane || profiles.length === 0) return [];
		const fills = [];
		for (let i = 0; i < profiles.length; i++) {
			const isHovered = hoveredProfile === i;
			const isSelected = selectedProfile === i;
			if (!isHovered && !isSelected) continue;

			const poly = profileToPolygon(profiles[i], entities, positions);
			if (poly.length < 3) continue;

			// Build THREE.Shape from polygon in sketch 2D coords
			const shape = new THREE.Shape();
			shape.moveTo(poly[0].x, poly[0].y);
			for (let j = 1; j < poly.length; j++) {
				shape.lineTo(poly[j].x, poly[j].y);
			}
			shape.closePath();

			const shapeGeo = new THREE.ShapeGeometry(shape);
			// Transform each vertex from sketch 2D to world 3D
			const posAttr = shapeGeo.getAttribute('position');
			for (let v = 0; v < posAttr.count; v++) {
				const sx = posAttr.getX(v);
				const sy = posAttr.getY(v);
				const w = sketchToWorld(sx, sy, plane);
				posAttr.setXYZ(v, w.x, w.y, w.z);
			}
			posAttr.needsUpdate = true;

			fills.push({
				index: i,
				geometry: shapeGeo,
				color: isSelected ? COLOR_PROFILE_SELECT : COLOR_PROFILE_HOVER,
				opacity: isSelected ? 0.15 : 0.1
			});
		}
		return fills;
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

		if (snap.type === 'on-entity' || snap.type === 'tangent' || snap.type === 'perpendicular') {
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
	const originGeometry = new THREE.SphereGeometry(0.05, 8, 8);
	const originMaterial = new THREE.MeshBasicMaterial({ color: COLOR_ORIGIN, depthTest: false, transparent: true, opacity: 0.6 });
	const axisXMaterial = new THREE.LineBasicMaterial({ color: COLOR_AXIS_X, depthTest: false, transparent: true, opacity: 0.4 });
	const axisYMaterial = new THREE.LineBasicMaterial({ color: COLOR_AXIS_Y, depthTest: false, transparent: true, opacity: 0.4 });

	// -- Sketch axes geometry --
	const AXIS_LENGTH = 50;
	let axisXGeo = $derived.by(() => {
		if (!plane) return null;
		const p1 = sketchToWorld(-AXIS_LENGTH, 0, plane);
		const p2 = sketchToWorld(AXIS_LENGTH, 0, plane);
		return new THREE.BufferGeometry().setFromPoints([p1, p2]);
	});
	let axisYGeo = $derived.by(() => {
		if (!plane) return null;
		const p1 = sketchToWorld(0, -AXIS_LENGTH, plane);
		const p2 = sketchToWorld(0, AXIS_LENGTH, plane);
		return new THREE.BufferGeometry().setFromPoints([p1, p2]);
	});
	let originWorld = $derived(plane ? sketchToWorld(0, 0, plane) : null);

	/**
	 * Callback to compute line distances for dashed materials.
	 * Must be called after the Line is created in the scene.
	 * @param {THREE.Line} lineObj
	 */
	function computeDashes(lineObj) {
		lineObj.computeLineDistances();
	}
</script>

{#if sm?.active && plane}
	<!-- Sketch axes -->
	{#if axisXGeo}
		<T.Line geometry={axisXGeo} material={axisXMaterial} renderOrder={7} />
	{/if}
	{#if axisYGeo}
		<T.Line geometry={axisYGeo} material={axisYMaterial} renderOrder={7} />
	{/if}
	{#if originWorld}
		<T.Mesh geometry={originGeometry} material={originMaterial}
			position={[originWorld.x, originWorld.y, originWorld.z]} renderOrder={7} />
	{/if}

	<!-- Profile fills (behind entities) -->
	{#each profileFills as fill (fill.index)}
		<T.Mesh geometry={fill.geometry} renderOrder={8}>
			<T.MeshBasicMaterial
				color={fill.color}
				depthTest={false}
				transparent
				opacity={fill.opacity}
				side={THREE.DoubleSide}
			/>
		</T.Mesh>
	{/each}

	<!-- Entity points -->
	{#each pointData as pt (pt.id)}
		<T.Mesh geometry={pointGeometry} position={[pt.world.x, pt.world.y, pt.world.z]} renderOrder={10}>
			<T.MeshBasicMaterial
				color={entityColor(pt.id)}
				depthTest={false}
				transparent={pt.construction}
				opacity={pt.construction ? 0.5 : 1}
			/>
		</T.Mesh>
	{/each}

	<!-- Entity lines -->
	{#each lineData as line (line.id)}
		{#if line.construction}
			<T.Line geometry={line.geometry} renderOrder={10} oncreate={computeDashes}>
				<T.LineDashedMaterial
					color={entityColor(line.id)}
					depthTest={false}
					dashSize={0.15}
					gapSize={0.08}
				/>
			</T.Line>
		{:else}
			<T.Line geometry={line.geometry} renderOrder={10}>
				<T.LineBasicMaterial color={entityColor(line.id)} depthTest={false} linewidth={1} />
			</T.Line>
		{/if}
	{/each}

	<!-- Entity circles -->
	{#each circleData as circle (circle.id)}
		{#if circle.construction}
			<T.Line geometry={circle.geometry} renderOrder={10} oncreate={computeDashes}>
				<T.LineDashedMaterial
					color={entityColor(circle.id)}
					depthTest={false}
					dashSize={0.15}
					gapSize={0.08}
				/>
			</T.Line>
		{:else}
			<T.Line geometry={circle.geometry} renderOrder={10}>
				<T.LineBasicMaterial color={entityColor(circle.id)} depthTest={false} linewidth={1} />
			</T.Line>
		{/if}
	{/each}

	<!-- Entity arcs -->
	{#each arcData as arc (arc.id)}
		{#if arc.construction}
			<T.Line geometry={arc.geometry} renderOrder={10} oncreate={computeDashes}>
				<T.LineDashedMaterial
					color={entityColor(arc.id)}
					depthTest={false}
					dashSize={0.15}
					gapSize={0.08}
				/>
			</T.Line>
		{:else}
			<T.Line geometry={arc.geometry} renderOrder={10}>
				<T.LineBasicMaterial color={entityColor(arc.id)} depthTest={false} linewidth={1} />
			</T.Line>
		{/if}
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
