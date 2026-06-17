<script>
	import { T, useThrelte } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import * as THREE from 'three';
	import {
		getSketchMode,
		getSketchEntities,
		getSketchPositions,
		getSketchSelection,
		getSketchHover,
		getSketchConstraints,
		getSketchSolveStatus,
		getExtractedProfiles,
		getSelectedProfileIndex,
		getHoveredProfileIndex,
		getOverConstrainedEntities,
		getFailedConstraintIndices,
		getGearDisplay,
		getGearRegistry
	} from '$lib/engine/store.svelte.js';
	import { getPreview, getSnapIndicator, getSnapCandidates } from './sketchToolState.svelte.js';
	import { buildSketchPlane, sketchToWorld } from './sketchCoords.js';
	import { profileToPolygon } from './profiles.js';
	import { sampleBSpline } from './bspline.js';

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
	const COLOR_FULLY_CONSTRAINED = 0x44cc88; // green, fully constrained (DOF=0)

	let sm = $derived(getSketchMode());
	let entities = $derived(getSketchEntities());
	let positions = $derived(getSketchPositions());
	let gearDisplay = $derived(getGearDisplay());

	// Merged view for the curve builders (lines/arcs/circles/splines): canonical
	// sketch geometry plus each gear's ephemeral display primitives. Gears are
	// stored as one compact `Gear` entity; their drawable curves live in
	// `gearDisplay` with ids in a non-colliding high range. Points are NOT merged
	// — gear vertices must not render as hundreds of draggable spheres.
	let renderEntities = $derived.by(() => {
		const merged = entities.filter(e => e.type !== 'Gear');
		for (const disp of gearDisplay.values()) merged.push(...disp.entities);
		return merged;
	});
	let renderPositions = $derived.by(() => {
		if (gearDisplay.size === 0) return positions;
		const merged = new Map(positions);
		for (const disp of gearDisplay.values()) {
			for (const [id, p] of disp.positions) merged.set(id, p);
		}
		return merged;
	});

	// Map each gear display-curve id → its owning `Gear` entity id, so the gear's
	// curves reflect the selection/hover state of the (single) gear entity.
	let gearCurveOwner = $derived.by(() => {
		const m = new Map();
		if (gearDisplay.size === 0) return m;
		const reg = getGearRegistry();
		for (const [gearId, disp] of gearDisplay) {
			const ownerId = reg.get(gearId)?.entityId;
			if (ownerId == null) continue;
			for (const e of disp.entities) m.set(e.id, ownerId);
		}
		return m;
	});
	let selection = $derived(getSketchSelection());
	let hoverEntity = $derived(getSketchHover());
	let constraints = $derived(getSketchConstraints());
	let profiles = $derived(getExtractedProfiles());
	let selectedProfile = $derived(getSelectedProfileIndex());
	let hoveredProfile = $derived(getHoveredProfileIndex());
	let overConstrained = $derived(getOverConstrainedEntities());

	let isFullyConstrained = $derived.by(() => {
		const status = getSketchSolveStatus();
		return status?.dof === 0 && status?.status !== 'error';
	});

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
		const entity = renderEntities.find(e => e.id === entityId);
		return entity?.construction ?? false;
	}

	/**
	 * Get entity color based on selection/hover/profile state.
	 * @param {number} entityId
	 * @returns {number}
	 */
	function entityColor(entityId) {
		// Gear curves inherit the selection/hover state of their owning gear entity.
		const selId = gearCurveOwner.get(entityId) ?? entityId;
		if (selection.has(selId)) return COLOR_SELECTED;
		if (hoverEntity === selId) return COLOR_HOVERED;
		if (overConstrained.has(entityId)) return COLOR_OVERCONSTRAINED;
		if (selectedProfileEntityIds.has(entityId)) return COLOR_PROFILE_SELECT;
		if (hoveredProfileEntityIds.has(entityId)) return COLOR_PROFILE_HOVER;
		if (isConstruction(entityId)) return COLOR_CONSTRUCTION;
		if (isFullyConstrained) return COLOR_FULLY_CONSTRAINED;
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
		return renderEntities
			.filter(e => e.type === 'Line')
			.map(e => {
				const p1 = renderPositions.get(e.start_id);
				const p2 = renderPositions.get(e.end_id);
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
		return renderEntities
			.filter(e => e.type === 'Circle')
			.map(e => {
				const center = renderPositions.get(e.center_id);
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
		return renderEntities
			.filter(e => e.type === 'Arc')
			.map(e => {
				const center = renderPositions.get(e.center_id);
				const startPt = renderPositions.get(e.start_id);
				const endPt = renderPositions.get(e.end_id);
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

	/**
	 * Build spline geometry (smooth curve through control points).
	 */
	let splineData = $derived.by(() => {
		if (!plane) return [];
		return renderEntities
			.filter(e => e.type === 'Spline')
			.map(e => {
				if (!e.point_ids || e.point_ids.length < 2) return null;
				const ctrlPts = e.point_ids
					.map(pid => renderPositions.get(pid))
					.filter(Boolean);
				if (ctrlPts.length < 2) return null;

				const sampled = sampleBSpline(ctrlPts, 48);
				const worldPts = sampled.map(p => sketchToWorld(p.x, p.y, plane));
				const geo = new THREE.BufferGeometry().setFromPoints(worldPts);
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

		if (preview.type === 'slot') {
			const { cx1, cy1, cx2, cy2, width } = preview.data;
			const dx = cx2 - cx1, dy = cy2 - cy1;
			const len = Math.sqrt(dx * dx + dy * dy);
			if (len < 0.000001) return null;
			const r = width / 2;
			const nx = -dy / len * r, ny = dx / len * r;
			const points = [];
			points.push(sketchToWorld(cx1 + nx, cy1 + ny, plane));
			points.push(sketchToWorld(cx2 + nx, cy2 + ny, plane));
			const arcSegs = 16;
			const baseAngle1 = Math.atan2(ny, nx);
			for (let i = 0; i <= arcSegs; i++) {
				const angle = baseAngle1 - Math.PI * (i / arcSegs);
				points.push(sketchToWorld(cx2 + Math.cos(angle) * r, cy2 + Math.sin(angle) * r, plane));
			}
			points.push(sketchToWorld(cx1 - nx, cy1 - ny, plane));
			const baseAngle2 = Math.atan2(-ny, -nx);
			for (let i = 0; i <= arcSegs; i++) {
				const angle = baseAngle2 - Math.PI * (i / arcSegs);
				points.push(sketchToWorld(cx1 + Math.cos(angle) * r, cy1 + Math.sin(angle) * r, plane));
			}
			return { type: 'line', geometry: new THREE.BufferGeometry().setFromPoints(points) };
		}

		if (preview.type === 'gear-preview') {
			const { polyline } = preview.data;
			if (!polyline || polyline.length < 2) return null;
			const worldPoints = polyline.map(p => sketchToWorld(p[0], p[1], plane));
			return { type: 'line', geometry: new THREE.BufferGeometry().setFromPoints(worldPoints) };
		}

		if (preview.type === 'trim-highlight') {
			const { points: pts } = preview.data;
			const worldPoints = pts.map(p => sketchToWorld(p.x, p.y, plane));
			return { type: 'trim', geometry: new THREE.BufferGeometry().setFromPoints(worldPoints) };
		}

		if (preview.type === 'fillet-preview') {
			const { cx, cy, radius, startAngle, endAngle } = preview.data;
			let end = endAngle;
			if (end <= startAngle) end += Math.PI * 2;
			if (end - startAngle > Math.PI) end -= Math.PI * 2;
			const segments = 32;
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

	const snapLabelMap = {
		'coincident': 'Coincident',
		'horizontal': 'Horizontal',
		'vertical': 'Vertical',
		'on-entity': 'On Entity',
		'tangent': 'Tangent',
		'perpendicular': 'Perpendicular',
		'midpoint': 'Midpoint',
		'quadrant': 'Quadrant',
		'origin': 'Origin',
		'reference': 'Reference'
	};

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

		if (snap.type === 'midpoint' || snap.type === 'quadrant' || snap.type === 'origin') {
			return { type: 'point', world: sketchToWorld(snap.x, snap.y, plane) };
		}

		if (snap.type === 'reference') {
			return { type: 'point', world: sketchToWorld(snap.x, snap.y, plane) };
		}

		return null;
	});

	let snapLabelData = $derived.by(() => {
		const snap = getSnapIndicator();
		if (!snap || !plane) return null;
		const text = snapLabelMap[snap.type];
		if (!text) return null;
		const world = sketchToWorld(snap.x + 0.00015, snap.y + 0.00015, plane);
		return { text, world };
	});

	let failedIndices = $derived(getFailedConstraintIndices());

	// Constraint label data (icons near constrained entities)
	let constraintLabels = $derived.by(() => {
		if (!plane) return [];
		const labels = [];
		const failed = failedIndices;
		const addLabel = (ci, text, world) => {
			labels.push({ text, world, failed: failed.has(ci) });
		};
		for (let ci = 0; ci < constraints.length; ci++) {
			const c = constraints[ci];
			// Skip temporary drag constraints
			if (c._isDrag) continue;

			if (c.type === 'Horizontal' || c.type === 'Vertical') {
				const entity = entities.find(e => e.id === c.entity);
				if (entity && entity.type === 'Line') {
					const p1 = positions.get(entity.start_id);
					const p2 = positions.get(entity.end_id);
					if (p1 && p2) {
						const mx = (p1.x + p2.x) / 2;
						const my = (p1.y + p2.y) / 2;
						const offsetX = c.type === 'Vertical' ? 0.0002 : 0;
						addLabel(ci, c.type === 'Horizontal' ? 'H' : 'V',
							sketchToWorld(mx + offsetX, my + 0.00015, plane));
					}
				}
			} else if (c.type === 'Parallel') {
				const l0 = entities.find(e => e.id === c.line_a);
				const l1 = entities.find(e => e.id === c.line_b);
				if (l0 && l1) {
					const p0s = positions.get(l0.start_id);
					const p0e = positions.get(l0.end_id);
					const p1s = positions.get(l1.start_id);
					const p1e = positions.get(l1.end_id);
					if (p0s && p0e && p1s && p1e) {
						const mx = (p0s.x + p0e.x + p1s.x + p1e.x) / 4;
						const my = (p0s.y + p0e.y + p1s.y + p1e.y) / 4;
						addLabel(ci, '||', sketchToWorld(mx, my + 0.00015, plane));
					}
				}
			} else if (c.type === 'Perpendicular') {
				const l0 = entities.find(e => e.id === c.line_a);
				const l1 = entities.find(e => e.id === c.line_b);
				if (l0 && l1) {
					const p0s = positions.get(l0.start_id);
					const p0e = positions.get(l0.end_id);
					const p1s = positions.get(l1.start_id);
					const p1e = positions.get(l1.end_id);
					if (p0s && p0e && p1s && p1e) {
						const mx = (p0s.x + p0e.x + p1s.x + p1e.x) / 4;
						const my = (p0s.y + p0e.y + p1s.y + p1e.y) / 4;
						addLabel(ci, '\u27c2', sketchToWorld(mx, my + 0.00015, plane));
					}
				}
			} else if (c.type === 'Equal' || c.type === 'EqualRadius') {
				const e0 = entities.find(e => e.id === c.entity_a);
				const e1 = entities.find(e => e.id === c.entity_b);
				if (e0 && e1) {
					for (const ent of [e0, e1]) {
						const pos = getEntityMidpoint(ent, positions);
						if (pos) {
							addLabel(ci, '=', sketchToWorld(pos.x, pos.y + 0.00015, plane));
						}
					}
				}
			} else if (c.type === 'Tangent') {
				const line = entities.find(e => e.id === c.line);
				const curve = entities.find(e => e.id === c.curve);
				if (line && curve) {
					const center = positions.get(curve.center_id);
					const ls = positions.get(line.start_id);
					const le = positions.get(line.end_id);
					if (center && ls && le) {
						const mx = (ls.x + le.x) / 2;
						const my = (ls.y + le.y) / 2;
						addLabel(ci, 'T', sketchToWorld(mx, my + 0.00015, plane));
					}
				}
			} else if (c.type === 'Coincident') {
				const posA = positions.get(c.point_a);
				if (posA) {
					addLabel(ci, '\u2022', sketchToWorld(posA.x + 0.0001, posA.y + 0.0001, plane));
				}
			} else if (c.type === 'Midpoint') {
				const pos = positions.get(c.point);
				if (pos) {
					addLabel(ci, 'M', sketchToWorld(pos.x, pos.y + 0.00015, plane));
				}
			} else if (c.type === 'WhereDragged') {
				const pos = positions.get(c.point);
				if (pos) {
					addLabel(ci, '\ud83d\udccc', sketchToWorld(pos.x + 0.0001, pos.y + 0.0001, plane));
				}
			} else if (c.type === 'Symmetric' || c.type === 'SymmetricH' || c.type === 'SymmetricV') {
				const posA = positions.get(c.point_a ?? c.entity_a);
				const posB = positions.get(c.point_b ?? c.entity_b);
				if (posA && posB) {
					const mx = (posA.x + posB.x) / 2;
					const my = (posA.y + posB.y) / 2;
					addLabel(ci, '\u2194', sketchToWorld(mx, my + 0.00015, plane));
				}
			} else if (c.type === 'OnEntity') {
				const pos = positions.get(c.point);
				if (pos) {
					addLabel(ci, '\u00d7', sketchToWorld(pos.x + 0.0001, pos.y + 0.0001, plane));
				}
			}
		}
		return labels;
	});

	/**
	 * Get the visual midpoint of an entity for constraint icon placement.
	 */
	function getEntityMidpoint(entity, positions) {
		if (entity.type === 'Line') {
			const p1 = positions.get(entity.start_id);
			const p2 = positions.get(entity.end_id);
			if (p1 && p2) return { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
		} else if (entity.type === 'Circle') {
			const center = positions.get(entity.center_id);
			if (center) return { x: center.x + (entity.radius || 1), y: center.y };
		} else if (entity.type === 'Arc') {
			const center = positions.get(entity.center_id);
			if (center) return center;
		} else if (entity.type === 'Spline' && entity.point_ids?.length > 0) {
			const midIdx = Math.floor(entity.point_ids.length / 2);
			const midPt = positions.get(entity.point_ids[midIdx]);
			if (midPt) return midPt;
		}
		return null;
	}

	// Under-constrained point detection (D7)
	const COLOR_UNDERCONSTRAINED = 0x33cccc; // cyan
	const underConstrainedGeo = new THREE.PlaneGeometry(0.0001, 0.0001);
	const underConstrainedMaterial = new THREE.MeshBasicMaterial({
		color: COLOR_UNDERCONSTRAINED, depthTest: false, transparent: true, opacity: 0.7
	});

	let underConstrainedPoints = $derived.by(() => {
		if (!plane) return [];
		const solveStatus = getSketchSolveStatus();
		if (!solveStatus || solveStatus.dof === 0) return [];

		// Build set of point IDs referenced by any constraint
		const constrainedIds = new Set();
		for (const c of constraints) {
			if (c._isDrag) continue;
			for (const key of ['point', 'point_a', 'point_b', 'entity_a', 'entity_b', 'entity']) {
				if (c[key] != null) {
					// Check if this ID is actually a point entity
					const ent = entities.find(e => e.id === c[key]);
					if (ent && ent.type === 'Point') constrainedIds.add(c[key]);
					// Also add implicit points from line/circle/arc entities
					if (ent && ent.type === 'Line') {
						constrainedIds.add(ent.start_id);
						constrainedIds.add(ent.end_id);
					}
					if (ent && (ent.type === 'Circle' || ent.type === 'Arc')) {
						constrainedIds.add(ent.center_id);
						if (ent.start_id) constrainedIds.add(ent.start_id);
						if (ent.end_id) constrainedIds.add(ent.end_id);
					}
				}
			}
		}

		// Find unconstrained points
		return entities
			.filter(e => e.type === 'Point' && !constrainedIds.has(e.id))
			.map(e => {
				const pos = positions.get(e.id);
				if (!pos) return null;
				return { id: e.id, world: sketchToWorld(pos.x, pos.y, plane) };
			})
			.filter(Boolean);
	});

	// Snap candidate preview data
	let snapCandidateData = $derived.by(() => {
		const candidates = getSnapCandidates();
		if (!candidates.length || !plane) return [];
		return candidates.map((c, i) => ({
			...c,
			key: `${c.type}-${c.entityId ?? c.sourceId ?? i}-${c.x.toFixed(6)}-${c.y.toFixed(6)}`,
			world: sketchToWorld(c.x, c.y, plane)
		}));
	});

	// Shared materials
	const previewMaterial = new THREE.LineBasicMaterial({ color: COLOR_PREVIEW, depthTest: false, transparent: true, opacity: 0.6 });
	const trimPreviewMaterial = new THREE.LineBasicMaterial({ color: 0xff6633, depthTest: false, transparent: true, opacity: 0.8 });
	const previewDashedMaterial = new THREE.LineDashedMaterial({ color: COLOR_PREVIEW, depthTest: false, transparent: true, opacity: 0.6, dashSize: 0.0001, gapSize: 0.00005 });
	const snapDashedMaterial = new THREE.LineDashedMaterial({ color: COLOR_SNAP, depthTest: false, transparent: true, opacity: 0.8, dashSize: 0.00008, gapSize: 0.00004 });
	const pointGeometry = new THREE.SphereGeometry(0.00006, 8, 8);
	const snapPointGeometry = new THREE.SphereGeometry(0.00008, 8, 8);
	const snapPointMaterial = new THREE.MeshBasicMaterial({ color: COLOR_SNAP, depthTest: false });
	const originGeometry = new THREE.SphereGeometry(0.00005, 8, 8);
	const originMaterial = new THREE.MeshBasicMaterial({ color: COLOR_ORIGIN, depthTest: false, transparent: true, opacity: 0.6 });
	const axisXMaterial = new THREE.LineBasicMaterial({ color: COLOR_AXIS_X, depthTest: false, transparent: true, opacity: 0.4 });
	const axisYMaterial = new THREE.LineBasicMaterial({ color: COLOR_AXIS_Y, depthTest: false, transparent: true, opacity: 0.4 });

	// Snap candidate preview materials (faint markers)
	const COLOR_SNAP_PREVIEW = 0x44cc44;   // green, matches snap
	const COLOR_MIDPOINT = 0xdd8833;       // orange
	const COLOR_QUADRANT = 0x33bbdd;       // cyan
	const COLOR_ORIGIN_SNAP = 0xffffff;    // white

	const snapCandidatePointMaterial = new THREE.MeshBasicMaterial({
		color: COLOR_SNAP_PREVIEW, depthTest: false, transparent: true, opacity: 0.3
	});
	const midpointGeometry = new THREE.CircleGeometry(0.00006, 3); // triangle (3 segments)
	const midpointMaterial = new THREE.MeshBasicMaterial({
		color: COLOR_MIDPOINT, depthTest: false, transparent: true, opacity: 0.5
	});
	const quadrantGeometry = new THREE.CircleGeometry(0.00006, 4); // diamond (4 segments)
	const quadrantMaterial = new THREE.MeshBasicMaterial({
		color: COLOR_QUADRANT, depthTest: false, transparent: true, opacity: 0.5
	});
	const COLOR_REFERENCE = 0x8866cc;      // purple, reference points from inactive sketches
	const referenceGeometry = new THREE.CircleGeometry(0.00006, 4); // diamond shape
	const referenceMaterial = new THREE.MeshBasicMaterial({
		color: COLOR_REFERENCE, depthTest: false, transparent: true, opacity: 0.5
	});

	const originSnapGeometry = new THREE.CircleGeometry(0.00008, 4);
	const originSnapMaterial = new THREE.MeshBasicMaterial({
		color: COLOR_ORIGIN_SNAP, depthTest: false, transparent: true, opacity: 0.4
	});

	// -- Sketch axes geometry --
	const AXIS_LENGTH = 0.05;
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
			position={[originWorld.x, originWorld.y, originWorld.z]} renderOrder={7}
			raycast={() => {}} />
	{/if}

	<!-- Profile fills (behind entities) -->
	{#each profileFills as fill (fill.index)}
		<T.Mesh geometry={fill.geometry} renderOrder={8} raycast={() => {}}>
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
		<T.Mesh geometry={pointGeometry} position={[pt.world.x, pt.world.y, pt.world.z]} renderOrder={10}
			raycast={() => {}}>
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
					dashSize={0.00015}
					gapSize={0.00008}
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
					dashSize={0.00015}
					gapSize={0.00008}
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
					dashSize={0.00015}
					gapSize={0.00008}
				/>
			</T.Line>
		{:else}
			<T.Line geometry={arc.geometry} renderOrder={10}>
				<T.LineBasicMaterial color={entityColor(arc.id)} depthTest={false} linewidth={1} />
			</T.Line>
		{/if}
	{/each}

	<!-- Entity splines -->
	{#each splineData as spline (spline.id)}
		{#if spline.construction}
			<T.Line geometry={spline.geometry} renderOrder={10} oncreate={computeDashes}>
				<T.LineDashedMaterial
					color={entityColor(spline.id)}
					depthTest={false}
					dashSize={0.00015}
					gapSize={0.00008}
				/>
			</T.Line>
		{:else}
			<T.Line geometry={spline.geometry} renderOrder={10}>
				<T.LineBasicMaterial color={entityColor(spline.id)} depthTest={false} linewidth={1} />
			</T.Line>
		{/if}
	{/each}

	<!-- Snap candidate preview markers (faint) — raycast disabled so clicks pass through to canvas -->
	{#each snapCandidateData as cand (cand.key)}
		{#if cand.type === 'point'}
			<T.Mesh geometry={pointGeometry} position={[cand.world.x, cand.world.y, cand.world.z]}
				renderOrder={9} material={snapCandidatePointMaterial} raycast={() => {}} />
		{:else if cand.type === 'midpoint'}
			<T.Mesh geometry={midpointGeometry} position={[cand.world.x, cand.world.y, cand.world.z]}
				renderOrder={9} material={midpointMaterial} raycast={() => {}} />
		{:else if cand.type === 'quadrant'}
			<T.Mesh geometry={quadrantGeometry} position={[cand.world.x, cand.world.y, cand.world.z]}
				renderOrder={9} material={quadrantMaterial} raycast={() => {}} />
		{:else if cand.type === 'origin'}
			<T.Mesh geometry={originSnapGeometry} position={[cand.world.x, cand.world.y, cand.world.z]}
				renderOrder={9} material={originSnapMaterial} raycast={() => {}} />
		{:else if cand.type === 'reference'}
			<T.Mesh geometry={referenceGeometry} position={[cand.world.x, cand.world.y, cand.world.z]}
				renderOrder={9} material={referenceMaterial} raycast={() => {}} />
		{/if}
	{/each}

	<!-- Preview geometry -->
	{#if previewGeo}
		{#if previewGeo.type === 'line'}
			<T.Line geometry={previewGeo.geometry} material={previewMaterial} renderOrder={10} />
		{:else if previewGeo.type === 'trim'}
			<T.Line geometry={previewGeo.geometry} material={trimPreviewMaterial} renderOrder={11} />
		{/if}
	{/if}

	<!-- Snap indicators — raycast disabled so clicks pass through to canvas -->
	{#if snapGeo}
		{#if snapGeo.type === 'point'}
			<T.Mesh geometry={snapPointGeometry} material={snapPointMaterial}
				position={[snapGeo.world.x, snapGeo.world.y, snapGeo.world.z]} renderOrder={11}
				raycast={() => {}} />
		{:else if snapGeo.type === 'dashed-line'}
			<T.Line geometry={snapGeo.geometry} material={snapDashedMaterial} renderOrder={11} />
		{/if}
	{/if}

	<!-- Snap text label -->
	{#if snapLabelData}
		<HTML position={[snapLabelData.world.x, snapLabelData.world.y, snapLabelData.world.z]} center={false} pointerEvents="none" wrapperClass="snap-html-wrapper">
			<span class="snap-label">{snapLabelData.text}</span>
		</HTML>
	{/if}

	<!-- Constraint labels -->
	{#each constraintLabels as label, i}
		<T.Mesh position={[label.world.x, label.world.y, label.world.z]} renderOrder={12}
			raycast={() => {}}>
			<T.PlaneGeometry args={[label.failed ? 0.00016 : 0.00012, label.failed ? 0.00016 : 0.00012]} />
			<T.MeshBasicMaterial color={label.failed ? COLOR_OVERCONSTRAINED : COLOR_DEFAULT} depthTest={false} transparent opacity={0.7} />
		</T.Mesh>
	{/each}

	<!-- Under-constrained point markers (D7) -->
	{#each underConstrainedPoints as ucp (ucp.id)}
		<T.Mesh geometry={underConstrainedGeo} material={underConstrainedMaterial}
			position={[ucp.world.x, ucp.world.y, ucp.world.z]} renderOrder={11}
			raycast={() => {}} />
	{/each}
{/if}

<style>
	/* Threlte <HTML> ignores pointerEvents prop in non-transform mode;
	   force pointer-events: none on the wrapper so clicks pass through to canvas */
	:global(.snap-html-wrapper) {
		pointer-events: none !important;
	}
	:global(.snap-html-wrapper *) {
		pointer-events: none !important;
	}

	:global(.snap-label) {
		background: rgba(30, 50, 30, 0.85);
		color: #44cc44;
		border: 1px solid #44cc44;
		border-radius: 3px;
		padding: 1px 5px;
		font-size: 10px;
		font-family: system-ui, sans-serif;
		white-space: nowrap;
		pointer-events: none;
	}
</style>
