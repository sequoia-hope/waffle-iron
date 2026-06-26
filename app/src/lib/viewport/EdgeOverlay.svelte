<script>
	import { T, useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import {
		getMeshes,
		getHoveredRef,
		getSelectedRefs,
		setHoveredRef,
		selectRef,
		geomRefEquals,
		getCameraObject,
		getSketchMode,
		isProjectToolActive,
		getSectionState,
		isBodyVisible,
		setRenderedEdgeBodyCount
	} from '$lib/engine/store.svelte.js';
	import { buildSectionClipPlane } from './sectionPlane.js';

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

	/** Pixel threshold for edge picking (how close cursor needs to be to an edge) */
	const EDGE_PICK_THRESHOLD = 6;

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
		// Set line precision in pixels (approx conversion from world to screen)
		_edgeRaycaster.params.Line = { threshold: EDGE_PICK_THRESHOLD * 0.01 };

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
	 * Check if a face is hit at the same screen position (face picks take priority).
	 * @param {number} clientX
	 * @param {number} clientY
	 * @returns {boolean}
	 */
	function isFaceHitAtPosition(clientX, clientY) {
		const camera = getCameraObject();
		if (!camera || !renderer) return false;

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		const mouse = new THREE.Vector2(
			((clientX - rect.left) / rect.width) * 2 - 1,
			-((clientY - rect.top) / rect.height) * 2 + 1
		);

		const raycaster = new THREE.Raycaster();
		raycaster.setFromCamera(mouse, camera);

		const meshObjects = [];
		const scene = camera.parent;
		if (scene) {
			scene.traverse((obj) => {
				if (/** @type {any} */ (obj).isMesh && obj.visible) {
					meshObjects.push(obj);
				}
			});
		}

		const intersections = raycaster.intersectObjects(meshObjects, false);
		return intersections.length > 0;
	}

	/**
	 * Handle pointer move for edge hover highlighting.
	 * Only fires if no face or vertex is under the cursor (they take priority).
	 * @param {MouseEvent} e
	 */
	function handleEdgePointerMove(e) {
		if (getSketchMode()?.active && !isProjectToolActive()) return;

		// Vertex picks take priority — if a vertex is already hovered, skip edge
		const currentRef = getHoveredRef();
		if (currentRef?.kind?.type === 'Vertex') return;

		// Only highlight edges if no face is currently hovered by CadModel
		// We check if a face is intersected — if so, CadModel handles hover
		if (isFaceHitAtPosition(e.clientX, e.clientY)) return;

		const edgeHit = pickEdgeAtScreen(e.clientX, e.clientY);
		if (edgeHit && edgeHit.ref) {
			setHoveredRef(edgeHit.ref);
		}
	}

	/**
	 * Handle click for edge selection.
	 * Only fires if no face or vertex is under the cursor.
	 * @param {MouseEvent} e
	 */
	function handleEdgeClick(e) {
		if (getSketchMode()?.active && !isProjectToolActive()) return;

		// Vertex clicks take priority
		const currentRef = getHoveredRef();
		if (currentRef?.kind?.type === 'Vertex') return;

		// Face clicks take priority — only select edge if no face hit
		if (isFaceHitAtPosition(e.clientX, e.clientY)) return;

		const edgeHit = pickEdgeAtScreen(e.clientX, e.clientY);
		if (edgeHit && edgeHit.ref) {
			const additive = e.shiftKey;
			selectRef(edgeHit.ref, additive);
		}
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
