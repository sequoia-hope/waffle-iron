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
		getHoveredRef,
		getReferenceSnapPoints,
		setReferenceSnapPoints,
		getProfilePickMode,
		getAxisPickMode,
		addProfileRegion,
		getSketchRegions,
		setRevolveAxis,
		getExtrudeDialogState,
		getRevolveDialogState,
		getInactiveGearDisplay,
		ensureInactiveGearsExpanded
	} from '$lib/engine/store.svelte.js';
	import { computeAxisFromSketchLine, computeAxisFromSketchCircle } from './axisUtils.js';
	import { buildSketchPlane, sketchToWorld } from './sketchCoords.js';
	import { extractProfiles, profileToPolygon, pointInPolygon, pointInRegion } from './profiles.js';
	import { sampleBSpline } from './bspline.js';

	const { renderer } = useThrelte();

	const COLOR_INACTIVE = 0x888888;
	const COLOR_SELECTED = 0xff8800;
	const COLOR_PROFILE_HOVER = 0x4488ff;
	const COLOR_PROFILE_SELECTED = 0x2266dd;
	const COLOR_ENTITY_HOVER = 0x44ddff;
	const COLOR_INACTIVE_POINT = 0x666688;
	const PICK_THRESHOLD_PX = 15;
	const ENTITY_PICK_THRESHOLD_2D = 0.002; // threshold in sketch 2D coords for entity picking

	/** @type {{ featureId: string, entityId: number } | null} */
	let hoveredAxisEntity = $state(null);

	let tree = $derived(getFeatureTree());
	let sm = $derived(getSketchMode());
	let selectedId = $derived(getSelectedFeatureId());
	let editingId = $derived(getEditingSketchFeatureId());

	/**
	 * Get all sketch features that should show inactive wireframes.
	 * During sketch mode: show wireframes from other sketches (not the one being edited).
	 * Outside sketch mode: show all visible inactive sketches.
	 */
	let sketchFeatures = $derived.by(() => {
		if (!tree?.features) return [];
		// Hide sketches past the rollback point (active_index): they belong to a
		// rolled-back portion of the timeline and should disappear from the scene.
		const ai = tree.active_index;
		return tree.features.filter((f, idx) =>
			(ai === null || ai === undefined || idx <= ai) &&
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
	let inactiveGears = $derived(getInactiveGearDisplay());

	// Expand any gear in a visible inactive sketch into the store's gear cache so
	// completed gear sketches render their teeth (a compact `Gear` entity carries
	// no drawable primitives on its own).
	$effect(() => {
		const specs = [];
		for (const feature of sketchFeatures) {
			for (const e of (feature.operation.sketch.entities || [])) {
				if (e.type === 'Gear') {
					specs.push({ key: `${feature.id}:${e.id}`, entityId: e.id, params: e.params });
				}
			}
		}
		ensureInactiveGearsExpanded(specs);
	});

	let sketchData = $derived.by(() => {
		const result = [];
		for (const feature of sketchFeatures) {
			const sketch = feature.operation.sketch;
			const origin = sketch.plane_origin || [0, 0, 0];
			const normal = sketch.plane_normal || [0, 0, 1];
			const plane = buildSketchPlane(origin, normal);

			// Reconstruct positions from Point entities. Prefer the constraint
			// solver's output (sketch.solved_positions, persisted by FinishSketch
			// and round-tripped through the .waffle file) over the raw drawn
			// entity.x/y: for a constrained sketch the two diverge, and reading
			// raw coords renders the finished sketch offset/wrong-sized. Fall back
			// to raw coords only when a point has no solved entry. Gear entities
			// are replaced by their cached display expansion (curves + positions).
			const solved = sketch.solved_positions || {};
			const solvedPos = (id) => solved[id] ?? solved[String(id)];
			const entities = [];
			const positions = new Map();
			for (const entity of (sketch.entities || [])) {
				if (entity.type === 'Gear') {
					const exp = inactiveGears.get(`${feature.id}:${entity.id}`);
					if (exp) {
						entities.push(...exp.entities);
						for (const [id, p] of exp.positions) positions.set(id, p);
					}
				} else {
					entities.push(entity);
					if (entity.type === 'Point' && entity.id != null) {
						const sp = solvedPos(entity.id);
						positions.set(entity.id, sp ? { x: sp[0], y: sp[1] } : { x: entity.x, y: entity.y });
					}
				}
			}
			const profiles = extractProfiles(entities, positions);

			result.push({ featureId: feature.id, sketch, plane, positions, entities, profiles });
		}
		return result;
	});

	/**
	 * Build wireframe geometry for each inactive sketch.
	 */
	let axisMode = $derived(getAxisPickMode());

	let sketchWireframes = $derived.by(() => {
		const result = [];
		const showConstruction = axisMode; // Show construction lines when picking axis
		for (const data of sketchData) {
			const { featureId, plane, positions, entities } = data;
			const isSelected = selectedId === featureId;
			const color = isSelected ? COLOR_SELECTED : COLOR_INACTIVE;
			const geometries = [];

			for (const entity of entities) {
				if (entity.construction && !showConstruction) continue;

				if (entity.type === 'Line') {
					const p1 = positions.get(entity.start_id);
					const p2 = positions.get(entity.end_id);
					if (!p1 || !p2) continue;
					const w1 = sketchToWorld(p1.x, p1.y, plane);
					const w2 = sketchToWorld(p2.x, p2.y, plane);
					geometries.push({
						type: 'line',
						entityId: entity.id,
						construction: !!entity.construction,
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
						entityId: entity.id,
						construction: !!entity.construction,
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
						entityId: entity.id,
						construction: !!entity.construction,
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
							entityId: entity.id,
							construction: !!entity.construction,
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
	 * Build point geometry for inactive sketch points (visible during sketch mode).
	 */
	let inactivePointData = $derived.by(() => {
		if (!sm?.active) return [];
		const pts = [];
		for (const data of sketchData) {
			for (const [id, pos] of data.positions) {
				const world = sketchToWorld(pos.x, pos.y, data.plane);
				pts.push({ key: `${data.featureId}-${id}`, world, featureId: data.featureId, pointId: id });
			}
		}
		return pts;
	});

	/**
	 * Build profile fill geometry for hovered inactive sketch profile.
	 */
	/**
	 * Collect all profiles that should be filled: hovered + selected (from dialog state).
	 */
	let profileFillTargets = $derived.by(() => {
		if (sm?.active) return [];
		const targets = [];

		// Hovered profile/region
		const hovered = getInactiveHoveredProfile();
		if (hovered) {
			targets.push({ featureId: hovered.featureId, profileIndex: hovered.profileIndex, region: hovered.region ?? null, color: 'hover' });
		}

		// Selected profiles/regions from extrude dialog regions
		const extrudeState = getExtrudeDialogState();
		if (extrudeState?.regions) {
			for (const r of extrudeState.regions) {
				if (r.type === 'sketchProfile' && r.sketchId) {
					const alreadyHovered = hovered && hovered.featureId === r.sketchId && hovered.profileIndex === (r.profileIndex ?? 0) && !r.region;
					if (!alreadyHovered) {
						targets.push({ featureId: r.sketchId, profileIndex: r.profileIndex ?? 0, region: r.region ?? null, color: 'selected' });
					}
				}
			}
		}

		// Selected profile from revolve dialog
		const revolveState = getRevolveDialogState();
		if (revolveState?.selectedProfile) {
			const sp = revolveState.selectedProfile;
			const sid = sp.sketchId ?? revolveState.sketchId;
			const alreadyHovered = hovered && hovered.featureId === sid && hovered.profileIndex === (sp.profileIndex ?? 0);
			if (!alreadyHovered) {
				targets.push({ featureId: sid, profileIndex: sp.profileIndex ?? 0, color: 'selected' });
			}
		}

		return targets;
	});

	let profileFills = $derived.by(() => {
		if (profileFillTargets.length === 0) return [];
		const fills = [];

		for (const target of profileFillTargets) {
			for (const data of sketchData) {
				if (data.featureId !== target.featureId) continue;

				let shape;
				if (target.region) {
					// Highlight the exact region: outer boundary minus holes.
					const outer = (target.region.outer ?? []).map(([x, y]) => ({ x, y }));
					if (outer.length < 3) continue;
					shape = new THREE.Shape();
					shape.moveTo(outer[0].x, outer[0].y);
					for (let j = 1; j < outer.length; j++) shape.lineTo(outer[j].x, outer[j].y);
					shape.closePath();
					for (const hole of target.region.holes ?? []) {
						if (hole.length < 3) continue;
						const path = new THREE.Path();
						path.moveTo(hole[0][0], hole[0][1]);
						for (let j = 1; j < hole.length; j++) path.lineTo(hole[j][0], hole[j][1]);
						path.closePath();
						shape.holes.push(path);
					}
				} else {
					const profile = data.profiles[target.profileIndex];
					if (!profile) continue;
					const poly = profileToPolygon(profile, data.entities, data.positions);
					if (poly.length < 3) continue;
					shape = new THREE.Shape();
					shape.moveTo(poly[0].x, poly[0].y);
					for (let j = 1; j < poly.length; j++) shape.lineTo(poly[j].x, poly[j].y);
					shape.closePath();
				}

				const shapeGeo = new THREE.ShapeGeometry(shape);
				const posAttr = shapeGeo.getAttribute('position');
				for (let v = 0; v < posAttr.count; v++) {
					const sx = posAttr.getX(v);
					const sy = posAttr.getY(v);
					const w = sketchToWorld(sx, sy, data.plane);
					posAttr.setXYZ(v, w.x, w.y, w.z);
				}
				posAttr.needsUpdate = true;

				const regionTag = target.region ? `r${(target.region.area ?? 0).toFixed(4)}` : `p${target.profileIndex}`;
				fills.push({
					key: `${data.featureId}-${regionTag}-${target.color}-${fills.length}`,
					geometry: shapeGeo,
					color: target.color === 'selected' ? COLOR_PROFILE_SELECTED : COLOR_PROFILE_HOVER,
					opacity: target.color === 'selected' ? 0.2 : 0.12
				});
			}
		}
		return fills;
	});

	/**
	 * Build tube geometry for the hovered axis entity to create a visible "bold" effect.
	 * WebGL linewidth > 1 doesn't work, so we use TubeGeometry for thickness.
	 */
	let hoveredEntityTubes = $derived.by(() => {
		if (!hoveredAxisEntity) return [];
		const tubes = [];
		for (const data of sketchData) {
			if (data.featureId !== hoveredAxisEntity.featureId) continue;
			const entity = data.entities.find(e => e.id === hoveredAxisEntity.entityId);
			if (!entity) continue;

			const { plane, positions } = data;
			const TUBE_RADIUS = 0.00008;
			const TUBE_SEGMENTS = 4;

			if (entity.type === 'Line') {
				const p1 = positions.get(entity.start_id);
				const p2 = positions.get(entity.end_id);
				if (!p1 || !p2) continue;
				const w1 = sketchToWorld(p1.x, p1.y, plane);
				const w2 = sketchToWorld(p2.x, p2.y, plane);
				const path = new THREE.LineCurve3(w1, w2);
				tubes.push({
					key: `tube-${data.featureId}-${entity.id}`,
					geometry: new THREE.TubeGeometry(path, 1, TUBE_RADIUS, TUBE_SEGMENTS, false)
				});
			} else if (entity.type === 'Circle') {
				const center = positions.get(entity.center_id);
				if (!center) continue;
				const pts = [];
				const segments = 48;
				for (let i = 0; i <= segments; i++) {
					const angle = (i / segments) * Math.PI * 2;
					const x = center.x + Math.cos(angle) * entity.radius;
					const y = center.y + Math.sin(angle) * entity.radius;
					pts.push(sketchToWorld(x, y, plane));
				}
				const path = new THREE.CatmullRomCurve3(pts, true);
				tubes.push({
					key: `tube-${data.featureId}-${entity.id}`,
					geometry: new THREE.TubeGeometry(path, segments, TUBE_RADIUS, TUBE_SEGMENTS, true)
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
				const pts = [];
				const segments = 32;
				for (let i = 0; i <= segments; i++) {
					const t = i / segments;
					const angle = startAngle + t * (endAngle - startAngle);
					const x = center.x + Math.cos(angle) * radius;
					const y = center.y + Math.sin(angle) * radius;
					pts.push(sketchToWorld(x, y, plane));
				}
				const path = new THREE.CatmullRomCurve3(pts, false);
				tubes.push({
					key: `tube-${data.featureId}-${entity.id}`,
					geometry: new THREE.TubeGeometry(path, segments, TUBE_RADIUS, TUBE_SEGMENTS, false)
				});
			}
		}
		return tubes;
	});

	// Reusable objects for raycasting to sketch plane
	const _raycaster = new THREE.Raycaster();
	const _mouse = new THREE.Vector2();
	const _planeObj = new THREE.Plane();
	const _intersection = new THREE.Vector3();
	const _projected = new THREE.Vector3();

	/**
	 * During sketch mode: pick inactive sketch points via screen-space proximity.
	 * Project picked point onto the current sketch plane and add as reference snap.
	 * @param {MouseEvent} e
	 */
	function handleSketchModePointPick(e) {
		const camera = getCameraObject();
		if (!camera || !renderer) return;

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();

		// Current sketch plane for projection
		const curOrigin = sm.origin;
		const curNormal = sm.normal;
		const curPlane = buildSketchPlane(curOrigin, curNormal);

		let closestDist = PICK_THRESHOLD_PX;
		let closestPt = null;

		for (const data of sketchData) {
			for (const [id, pos] of data.positions) {
				const world = sketchToWorld(pos.x, pos.y, data.plane);
				// Project to screen
				_projected.copy(world).project(camera);
				const sx = rect.left + ((_projected.x + 1) / 2) * rect.width;
				const sy = rect.top + ((1 - _projected.y) / 2) * rect.height;

				const dx = e.clientX - sx;
				const dy = e.clientY - sy;
				const dist = Math.sqrt(dx * dx + dy * dy);

				if (dist < closestDist) {
					closestDist = dist;
					closestPt = { world, featureId: data.featureId, pointId: id };
				}
			}
		}

		if (closestPt) {
			// Project the 3D point onto the current sketch plane's 2D space
			const rx = closestPt.world.x - curOrigin[0];
			const ry = closestPt.world.y - curOrigin[1];
			const rz = closestPt.world.z - curOrigin[2];
			const u = rx * curPlane.xAxis.x + ry * curPlane.xAxis.y + rz * curPlane.xAxis.z;
			const v = rx * curPlane.yAxis.x + ry * curPlane.yAxis.y + rz * curPlane.yAxis.z;

			const sourceId = `${closestPt.featureId}:${closestPt.pointId}`;
			const wp = [closestPt.world.x, closestPt.world.y, closestPt.world.z];

			// Add to reference snap points if not already present
			const existing = getReferenceSnapPoints();
			const alreadyHas = existing.some(p => p.sourceId === sourceId);
			if (!alreadyHas) {
				setReferenceSnapPoints([...existing, { x: u, y: v, sourceId, worldPos: wp }]);
			}
		}
	}

	/**
	 * Map an analytical region (one whose boundary equals a whole-loop profile)
	 * back to that profile's index in the JS profile list, so selecting it uses
	 * the existing analytical extrude path. Returns 0 for genuine sub-regions
	 * (their profile_index is unused — they extrude from explicit geometry).
	 * @param {{ profiles: Array<object> }} data
	 * @param {{ profile_entity_ids?: number[] }} region
	 * @returns {number}
	 */
	function resolveRegionProfileIndex(data, region) {
		const ids = region.profile_entity_ids;
		if (!ids || ids.length === 0) return 0;
		const want = [...ids].sort((a, b) => a - b).join(',');
		for (let i = 0; i < data.profiles.length; i++) {
			const got = [...(data.profiles[i].entityIds ?? [])].sort((a, b) => a - b).join(',');
			if (got === want) return i;
		}
		return 0;
	}

	/**
	 * Hit-test cursor against inactive sketch profiles (non-sketch mode)
	 * or pick inactive sketch points (sketch mode).
	 * @param {MouseEvent} e
	 */
	function handlePointerMove(e) {
		if (sm?.active) {
			// In sketch mode: pick inactive sketch points for cross-plane snap
			if (getInactiveHoveredProfile()) setInactiveHoveredProfile(null);
			if (hoveredAxisEntity) hoveredAxisEntity = null;
			handleSketchModePointPick(e);
			return;
		}

		// Don't hover profiles when a face or edge is under the cursor,
		// UNLESS a pick mode is active (profile or axis picking needs priority)
		const hasPickMode = getProfilePickMode() || getAxisPickMode();
		if (getHoveredRef() && !hasPickMode) {
			if (getInactiveHoveredProfile()) setInactiveHoveredProfile(null);
			if (hoveredAxisEntity) hoveredAxisEntity = null;
			return;
		}

		const camera = getCameraObject();
		if (!camera || !renderer) {
			if (getInactiveHoveredProfile()) setInactiveHoveredProfile(null);
			if (hoveredAxisEntity) hoveredAxisEntity = null;
			return;
		}

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		_mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
		_mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
		_raycaster.setFromCamera(_mouse, camera);

		// Entity hover for axis pick mode
		let foundEntity = false;
		if (getAxisPickMode()) {
			for (const data of sketchData) {
				const origin = data.sketch.plane_origin || [0, 0, 0];
				const normal = data.sketch.plane_normal || [0, 0, 1];
				const n = new THREE.Vector3(normal[0], normal[1], normal[2]).normalize();
				const o = new THREE.Vector3(origin[0], origin[1], origin[2]);
				_planeObj.setFromNormalAndCoplanarPoint(n, o);

				if (!_raycaster.ray.intersectPlane(_planeObj, _intersection)) continue;

				const rel = _intersection.clone().sub(o);
				const sx = rel.dot(data.plane.xAxis);
				const sy = rel.dot(data.plane.yAxis);

				let bestDist = ENTITY_PICK_THRESHOLD_2D;
				let bestEntity = null;

				for (const entity of data.entities) {
					if (entity.type === 'Line') {
						const p1 = data.positions.get(entity.start_id);
						const p2 = data.positions.get(entity.end_id);
						if (!p1 || !p2) continue;
						const dist = pointToSegmentDist2D(sx, sy, p1.x, p1.y, p2.x, p2.y);
						if (dist < bestDist) { bestDist = dist; bestEntity = entity; }
					} else if (entity.type === 'Circle') {
						const center = data.positions.get(entity.center_id);
						if (!center) continue;
						const dx = sx - center.x;
						const dy = sy - center.y;
						const distToCenter = Math.sqrt(dx * dx + dy * dy);
						const dist = Math.abs(distToCenter - entity.radius);
						if (dist < bestDist) { bestDist = dist; bestEntity = entity; }
					}
				}

				if (bestEntity) {
					hoveredAxisEntity = { featureId: data.featureId, entityId: bestEntity.id };
					foundEntity = true;
					break;
				}
			}
		}
		if (!foundEntity && hoveredAxisEntity) {
			hoveredAxisEntity = null;
		}

		// Check each sketch. For extrude, prefer the smallest Rust-computed
		// region under the cursor (so sub-regions of overlapping shapes are
		// reachable); fall back to whole-loop profiles when no regions are
		// available (revolve, gear sketches, or before regions arrive).
		const useRegions = getProfilePickMode()?.target === 'extrude';
		for (const data of sketchData) {
			const regions = useRegions ? getSketchRegions(data.featureId) : null;
			const hasRegions = regions && regions.length > 0;
			if (!hasRegions && data.profiles.length === 0) continue;

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

			if (hasRegions) {
				// Smallest-area region whose interior contains the cursor.
				let best = null;
				for (const region of regions) {
					if (pointInRegion(sx, sy, region) && (!best || region.area < best.area)) {
						best = region;
					}
				}
				if (best) {
					setInactiveHoveredProfile({
						featureId: data.featureId,
						profileIndex: resolveRegionProfileIndex(data, best),
						region: best
					});
					return;
				}
				continue;
			}

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

	// Shared materials for inactive points
	const inactivePointGeometry = new THREE.SphereGeometry(0.00004, 6, 6);
	const inactivePointMaterial = new THREE.MeshBasicMaterial({
		color: COLOR_INACTIVE_POINT,
		depthTest: false,
		transparent: true,
		opacity: 0.5
	});

	/**
	 * Handle click for profile picking (extrude/revolve) and axis picking (revolve).
	 * Uses window listener like SketchInteraction does.
	 * @param {PointerEvent} e
	 */
	function handlePointerDown(e) {
		if (e.button !== 0) return; // Left click only
		if (sm?.active) return;

		const pickMode = getProfilePickMode();
		const axisMode = getAxisPickMode();
		if (!pickMode && !axisMode) return;

		const camera = getCameraObject();
		if (!camera || !renderer) return;

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		_mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
		_mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
		_raycaster.setFromCamera(_mouse, camera);

		// Profile picking: check if cursor is over a profile/region
		if (pickMode) {
			const hovered = getInactiveHoveredProfile();
			if (hovered) {
				addProfileRegion(hovered.featureId, hovered.profileIndex, hovered.region ?? null);
				return;
			}
		}

		// Axis picking from sketch entities: raycast to each sketch plane and find nearest entity
		if (axisMode) {
			for (const data of sketchData) {
				const origin = data.sketch.plane_origin || [0, 0, 0];
				const normal = data.sketch.plane_normal || [0, 0, 1];
				const n = new THREE.Vector3(normal[0], normal[1], normal[2]).normalize();
				const o = new THREE.Vector3(origin[0], origin[1], origin[2]);
				_planeObj.setFromNormalAndCoplanarPoint(n, o);

				if (!_raycaster.ray.intersectPlane(_planeObj, _intersection)) continue;

				// Transform to sketch 2D
				const rel = _intersection.clone().sub(o);
				const sx = rel.dot(data.plane.xAxis);
				const sy = rel.dot(data.plane.yAxis);

				// Find nearest line or circle entity (including construction)
				let bestDist = ENTITY_PICK_THRESHOLD_2D;
				let bestEntity = null;

				for (const entity of data.entities) {
					if (entity.type === 'Line') {
						const p1 = data.positions.get(entity.start_id);
						const p2 = data.positions.get(entity.end_id);
						if (!p1 || !p2) continue;
						const dist = pointToSegmentDist2D(sx, sy, p1.x, p1.y, p2.x, p2.y);
						if (dist < bestDist) {
							bestDist = dist;
							bestEntity = entity;
						}
					} else if (entity.type === 'Circle') {
						const center = data.positions.get(entity.center_id);
						if (!center) continue;
						const dx = sx - center.x;
						const dy = sy - center.y;
						const distToCenter = Math.sqrt(dx * dx + dy * dy);
						const dist = Math.abs(distToCenter - entity.radius);
						if (dist < bestDist) {
							bestDist = dist;
							bestEntity = entity;
						}
					}
				}

				if (bestEntity) {
					let axis = null;
					let label = '';
					if (bestEntity.type === 'Line') {
						const p1 = data.positions.get(bestEntity.start_id);
						const p2 = data.positions.get(bestEntity.end_id);
						if (p1 && p2) {
							axis = computeAxisFromSketchLine(
								{ start: [p1.x, p1.y], end: [p2.x, p2.y] },
								origin, normal
							);
							label = bestEntity.construction ? `Line ${bestEntity.id} (constr.)` : `Line ${bestEntity.id}`;
						}
					} else if (bestEntity.type === 'Circle') {
						const center = data.positions.get(bestEntity.center_id);
						if (center) {
							axis = computeAxisFromSketchCircle(
								{ center: [center.x, center.y] },
								origin, normal
							);
							label = `Circle ${bestEntity.id}`;
						}
					}
					if (axis) {
						setRevolveAxis(axis.origin, axis.direction, label);
						return;
					}
				}
			}
		}
	}

	/**
	 * Point-to-segment distance in 2D.
	 */
	function pointToSegmentDist2D(px, py, x1, y1, x2, y2) {
		const dx = x2 - x1;
		const dy = y2 - y1;
		const lenSq = dx * dx + dy * dy;
		if (lenSq < 1e-20) return Math.sqrt((px - x1) ** 2 + (py - y1) ** 2);
		let t = ((px - x1) * dx + (py - y1) * dy) / lenSq;
		t = Math.max(0, Math.min(1, t));
		const projX = x1 + t * dx;
		const projY = y1 + t * dy;
		return Math.sqrt((px - projX) ** 2 + (py - projY) ** 2);
	}

	onMount(() => {
		const canvas = renderer?.domElement;
		if (!canvas) return;
		canvas.addEventListener('pointermove', handlePointerMove);
		window.addEventListener('pointerdown', handlePointerDown);
		return () => {
			canvas.removeEventListener('pointermove', handlePointerMove);
			window.removeEventListener('pointerdown', handlePointerDown);
		};
	});
</script>

{#each sketchWireframes as wireframe (wireframe.featureId)}
	{#each wireframe.geometries as geo, i}
		{@const isHovered = hoveredAxisEntity && hoveredAxisEntity.featureId === wireframe.featureId && hoveredAxisEntity.entityId === geo.entityId}
		<T.Line geometry={geo.geometry} renderOrder={3} userData={{ waffleType: 'sketch' }}>
			<T.LineBasicMaterial
				color={isHovered ? COLOR_ENTITY_HOVER : (geo.construction ? 0x666688 : wireframe.color)}
				depthTest={true}
				transparent
				opacity={isHovered ? 1.0 : (geo.construction ? 0.35 : (sm?.active ? 0.3 : 0.6))}
			/>
		</T.Line>
	{/each}
{/each}

<!-- Bold tube overlay for hovered axis entity -->
{#each hoveredEntityTubes as tube (tube.key)}
	<T.Mesh geometry={tube.geometry} renderOrder={4}>
		<T.MeshBasicMaterial
			color={COLOR_ENTITY_HOVER}
			transparent
			opacity={0.6}
			depthTest={true}
		/>
	</T.Mesh>
{/each}

<!-- Inactive sketch points (shown during sketch mode) -->
{#each inactivePointData as pt (pt.key)}
	<T.Mesh geometry={inactivePointGeometry} material={inactivePointMaterial}
		position={[pt.world.x, pt.world.y, pt.world.z]}
		renderOrder={6} raycast={() => {}} />
{/each}

{#each profileFills as fill (fill.key)}
	<!-- raycast disabled: highlight fills are decoration, never a pickable face -->
	<T.Mesh geometry={fill.geometry} renderOrder={2} raycast={() => {}}>
		<T.MeshBasicMaterial
			color={fill.color}
			transparent
			opacity={fill.opacity}
			side={THREE.DoubleSide}
			depthTest={true}
		/>
	</T.Mesh>
{/each}
