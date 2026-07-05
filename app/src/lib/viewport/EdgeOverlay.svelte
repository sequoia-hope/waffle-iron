<script>
	import { T, useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import {
		getMeshes,
		getHoveredRef,
		getSelectedRefs,
		selectRef,
		geomRefEquals,
		getCameraObject,
		getSketchMode,
		getSectionState,
		isBodyVisible,
		setRenderedEdgeBodyCount,
		isBodyPickingEnabled,
		proposeHoverRef,
		getSketchHover
	} from '$lib/engine/store.svelte.js';
	import { buildSectionClipPlane } from './sectionPlane.js';
	import { worldPerPixel, faceOccludes, OCCLUSION_DEPTH_EPS_PX } from './picking.js';

	const { renderer } = useThrelte();

	const DEFAULT_EDGE_COLOR = new THREE.Color(0x222233);
	const HOVER_EDGE_COLOR = new THREE.Color(0x66aaff);
	const SELECTED_EDGE_COLOR = new THREE.Color(0x44aaff);

	const baseMaterialProps = {
		linewidth: 1,
		depthTest: true,
		polygonOffset: true,
		polygonOffsetFactor: -0.5,
		polygonOffsetUnits: -0.5
	};

	const fallbackMaterial = new THREE.LineBasicMaterial({
		color: DEFAULT_EDGE_COLOR,
		...baseMaterialProps
	});

	/** Screen-pixel threshold for edge picking (how close the cursor must be to an
	 *  edge's projection). Converted to world units per frame — see worldPerPixel. */
	const EDGE_PICK_THRESHOLD_PX = 6;

	// Reusable raycaster for edge picking
	const _edgeRaycaster = new THREE.Raycaster();
	const _edgeMouse = new THREE.Vector2();

	/**
	 * Build line segments geometry from edge render data.
	 * If edge ranges exist, add groups for per-edge material assignment.
	 */
	function buildEdgeGeometry(edgeData) {
		if (!edgeData || !edgeData.vertices || edgeData.vertices.length === 0) return null;
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(edgeData.vertices, 3));

		if (edgeData.ranges && edgeData.ranges.length > 0) {
			geo.clearGroups();
			for (let i = 0; i < edgeData.ranges.length; i++) {
				const range = edgeData.ranges[i];
				geo.addGroup(range.start_index, range.end_index - range.start_index, i);
			}
		}

		return geo;
	}

	/**
	 * Build materials array for edge ranges based on hover/selection state.
	 */
	function buildEdgeMaterials(ranges, hoveredRef, selectedRefs) {
		if (!ranges || ranges.length === 0) {
			return [fallbackMaterial];
		}

		return ranges.map((range) => {
			const ref = range.geom_ref;
			let color = DEFAULT_EDGE_COLOR;

			if (selectedRefs.some((r) => geomRefEquals(r, ref))) {
				color = SELECTED_EDGE_COLOR;
			} else if (hoveredRef && geomRefEquals(hoveredRef, ref)) {
				color = HOVER_EDGE_COLOR;
			}

			return new THREE.LineBasicMaterial({
				color,
				...baseMaterialProps
			});
		});
	}

	/**
	 * Find the edge GeomRef closest to a screen position by raycasting against
	 * LineSegments objects. Returns null if no edge is within EDGE_PICK_THRESHOLD.
	 * @param {number} clientX
	 * @param {number} clientY
	 * @returns {{ ref: any, distance: number } | null}
	 */
	function pickEdgeAtScreen(clientX, clientY) {
		const camera = getCameraObject();
		if (!camera || !renderer) return null;

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		_edgeMouse.x = ((clientX - rect.left) / rect.width) * 2 - 1;
		_edgeMouse.y = -((clientY - rect.top) / rect.height) * 2 + 1;

		_edgeRaycaster.setFromCamera(_edgeMouse, camera);
		// Line precision is a world-space distance; calibrate it from the screen
		// pixel threshold for the current camera/zoom so an edge is hover-eligible
		// only within a few px of its projection at ANY part scale (root-cause fix
		// for the absolute-world 0.06 threshold that made every pixel "near" an
		// edge on small parts).
		const wpp = worldPerPixel(camera, rect.height, camera.position?.length?.());
		_edgeRaycaster.params.Line = { threshold: EDGE_PICK_THRESHOLD_PX * wpp };

		// Collect LineSegments from the scene
		const lineObjects = [];
		const scene = camera.parent;
		if (scene) {
			scene.traverse((obj) => {
				if (/** @type {any} */ (obj).isLineSegments) {
					lineObjects.push(obj);
				}
			});
		}

		if (lineObjects.length === 0) return null;

		const intersections = _edgeRaycaster.intersectObjects(lineObjects, false);
		if (intersections.length === 0) return null;

		// Find the edge range containing this intersection
		const hit = intersections[0];
		const hitIndex = hit.index;
		if (hitIndex == null) return null;

		// Find which edge range owns this vertex index
		const meshData = getMeshes();
		if (!meshData) return null;

		for (const mesh of meshData) {
			if (!mesh.edges || !mesh.edges.ranges) continue;
			for (const range of mesh.edges.ranges) {
				// The hit index is a vertex index in the LineSegments geometry
				// Each segment is 2 vertices, ranges use vertex indices
				if (hitIndex >= range.start_index && hitIndex < range.end_index) {
					return { ref: range.geom_ref, distance: hit.distance };
				}
			}
		}

		return null;
	}

	/**
	 * True when a face is strictly closer than the edge hit (invariant I2) — the
	 * shared, screen-calibrated occlusion rule (see picking.js).
	 * @param {number} clientX
	 * @param {number} clientY
	 * @param {number} edgeDist
	 * @returns {boolean}
	 */
	function edgeOccludedByFace(clientX, clientY, edgeDist) {
		return faceOccludes(getCameraObject(), renderer, clientX, clientY, edgeDist, OCCLUSION_DEPTH_EPS_PX);
	}

	/**
	 * Handle pointer move for edge hover highlighting.
	 * Only fires if no face or vertex is under the cursor (they take priority).
	 * @param {MouseEvent} e
	 */
	function handleEdgePointerMove(e) {
		if (!isBodyPickingEnabled()) return;
		// Invariant I1: a sketch entity under the pointer wins over the body.
		if (getSketchMode()?.active && getSketchHover() != null) return;

		const edgeHit = pickEdgeAtScreen(e.clientX, e.clientY);
		if (!edgeHit || !edgeHit.ref) return;

		// Invariant I2: occlusion, not existence — only a face strictly nearer
		// than the edge suppresses it.
		if (edgeOccludedByFace(e.clientX, e.clientY, edgeHit.distance)) return;

		// Invariant I3: propose the edge for this pixel; a Vertex proposal for the
		// same pixel supersedes it, a Face proposal does not.
		proposeHoverRef(edgeHit.ref, e.clientX, e.clientY);
	}

	/**
	 * Handle click for edge selection.
	 * Only fires if no face or vertex is under the cursor.
	 * @param {MouseEvent} e
	 */
	function handleEdgeClick(e) {
		if (!isBodyPickingEnabled()) return;
		// Invariant I1: a sketch entity under the pointer wins over the body.
		if (getSketchMode()?.active && getSketchHover() != null) return;

		// Invariant I3: a hovered Vertex outranks the edge — defer to it.
		if (getHoveredRef()?.kind?.type === 'Vertex') return;

		const edgeHit = pickEdgeAtScreen(e.clientX, e.clientY);
		if (!edgeHit || !edgeHit.ref) return;

		// Invariant I2: an edge occluded by a nearer face is not selectable.
		if (edgeOccludedByFace(e.clientX, e.clientY, edgeHit.distance)) return;

		selectRef(edgeHit.ref, e.shiftKey);
	}

	// Derive edge geometries from mesh state
	let edgeGeometries = $derived.by(() => {
		const meshData = getMeshes();
		if (!meshData) return [];
		return meshData
			// Hiding a body hides its edges too (mirrors CadModel's face filter).
			.filter((m) => isBodyVisible(m.bodyId))
			.filter((m) => m.edges && m.edges.vertices && m.edges.vertices.length > 0)
			.map((m) => ({
				geometry: buildEdgeGeometry(m.edges),
				ranges: m.edges.ranges || [],
				featureId: m.featureId,
				bodyId: m.bodyId
			}))
			.filter((e) => e.geometry !== null);
	});

	// Publish the real rendered edge-body count for GUI test introspection.
	$effect(() => setRenderedEdgeBodyCount(edgeGeometries.length));

	// Build material arrays reactively based on hover/selection state
	let edgeMaterials = $derived.by(() => {
		const hRef = getHoveredRef();
		const sRefs = getSelectedRefs();
		return edgeGeometries.map((e) => buildEdgeMaterials(e.ranges, hRef, sRefs));
	});

	// Capped section view: clip edges on the removed side with the SAME plane
	// CadModel uses. Re-applies whenever the materials rebuild (hover/selection)
	// or the section plane changes; cleared to [] when inactive.
	let sectionClipPlane = $derived.by(() => {
		const s = getSectionState();
		if (!s.active || !s.plane) return null;
		return buildSectionClipPlane(s.plane, s.flipped, s.offset);
	});

	$effect(() => {
		const plane = sectionClipPlane;
		const planes = plane ? [plane] : [];
		for (const matArr of edgeMaterials) {
			if (!matArr) continue;
			for (const mat of matArr) {
				if (!mat) continue;
				mat.clippingPlanes = planes;
				mat.needsUpdate = true;
			}
		}
		fallbackMaterial.clippingPlanes = planes;
		fallbackMaterial.needsUpdate = true;
	});

	onMount(() => {
		const canvas = renderer?.domElement;
		if (!canvas) return;

		canvas.addEventListener('pointermove', handleEdgePointerMove);
		canvas.addEventListener('click', handleEdgeClick);

		return () => {
			canvas.removeEventListener('pointermove', handleEdgePointerMove);
			canvas.removeEventListener('click', handleEdgeClick);
		};
	});
</script>

{#each edgeGeometries as edge, i (edge.bodyId)}
	<T.LineSegments
		geometry={edge.geometry}
		material={edgeMaterials[i]?.length > 1 ? edgeMaterials[i] : edgeMaterials[i]?.[0]}
		renderOrder={1}
	/>
{/each}
