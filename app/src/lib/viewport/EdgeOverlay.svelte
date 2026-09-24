<script>
	import { T, useThrelte, useTask } from '@threlte/core';
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
	import { getTheme } from '$lib/ui/theme.svelte.js';
	import { getColorVersion } from '$lib/ui/settings.svelte.js';
	import { worldPerPixel, faceOccludes, OCCLUSION_DEPTH_EPS_PX } from './picking.js';
	import { placementProps } from './placement.js';

	const { renderer } = useThrelte();

	/**
	 * Resolve a CSS custom property on <html> to a THREE.Color, falling back to
	 * `fallbackHex` when the var is unset or we're off-DOM (SSR). Mirrors the
	 * helper in CadModel.svelte.
	 * @param {string} name
	 * @param {number} fallbackHex
	 */
	function cssColor(name, fallbackHex) {
		if (typeof document !== 'undefined') {
			const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
			if (v) return new THREE.Color(v);
		}
		return new THREE.Color(fallbackHex);
	}

	// Unselected edge color is theme-driven (see --model-edge-color in app.css)
	// and customizable from Settings -> Appearance. Reading getTheme() and
	// getColorVersion() makes this recompute on a theme switch or a per-token
	// override; edgeMaterials reads it, so the material arrays rebuild with it.
	let DEFAULT_EDGE_COLOR = $derived.by(() => {
		void getTheme(); void getColorVersion();
		return cssColor('--model-edge-color', 0xf4f7fb);
	});
	const HOVER_EDGE_COLOR = new THREE.Color(0x66aaff);
	const SELECTED_EDGE_COLOR = new THREE.Color(0x44aaff);

	const baseMaterialProps = {
		linewidth: 1,
		depthTest: true
	};

	// Edges lie exactly ON the faces they bound, so an unbiased depth test is a
	// coin flip per pixel: the line and the triangle interpolate depth
	// differently, and the edge renders as a dashed line that alternates with the
	// solid. `polygonOffset` cannot fix it — WebGL applies polygon offset to
	// filled triangles only, never to GL_LINES. Instead each edge vertex is
	// pulled toward the camera ALONG ITS VIEW RAY (so its screen position is
	// unchanged) by a screen-constant EDGE_DEPTH_BIAS_PX. Edges on visible faces
	// then always win; edges genuinely behind the part are hidden by far more
	// than a couple of pixels and stay hidden.
	const EDGE_DEPTH_BIAS_PX = 2;
	// CSS-pixel height of the canvas, shared by every edge material's shader and
	// refreshed each frame (the bias is measured in screen pixels).
	const viewportHeightUniform = { value: 1 };

	/** @param {THREE.LineBasicMaterial} mat */
	function withEdgeDepthBias(mat) {
		mat.onBeforeCompile = (shader) => {
			shader.uniforms.edgeViewportHeight = viewportHeightUniform;
			shader.vertexShader = shader.vertexShader
				.replace('void main() {', 'uniform float edgeViewportHeight;\nvoid main() {')
				.replace(
					'#include <project_vertex>',
					`#include <project_vertex>
					{
						// World units per pixel at this vertex's depth.
						float edgeWpp = 2.0 / ( projectionMatrix[ 1 ][ 1 ] * edgeViewportHeight );
						vec4 edgeMv = mvPosition;
						// Orthographic iff the projection has no perspective divide
						// (the built-in isOrthographic uniform is not uploaded for
						// LineBasicMaterial, so it cannot be trusted here).
						if ( projectionMatrix[ 3 ][ 3 ] == 1.0 ) {
							edgeMv.z += ${EDGE_DEPTH_BIAS_PX.toFixed(1)} * edgeWpp;
						} else {
							float edgeDist = -mvPosition.z;
							float edgePull = min( ${EDGE_DEPTH_BIAS_PX.toFixed(1)} * edgeWpp * edgeDist, 0.5 * edgeDist );
							edgeMv.xyz += normalize( -mvPosition.xyz ) * edgePull;
						}
						// mvPosition itself is left untouched so section clipping
						// (vClipPosition) still cuts at the true edge position.
						gl_Position = projectionMatrix * edgeMv;
					}`
				);
		};
		mat.customProgramCacheKey = () => 'waffle-edge-depth-bias';
		return mat;
	}

	// Shared material for edge data that carries no per-edge ranges. Its color is
	// kept on the theme by the $effect below (it is mutated, not rebuilt, because
	// the section-clipping effect holds the same instance).
	const fallbackMaterial = withEdgeDepthBias(
		new THREE.LineBasicMaterial({
			color: 0xf4f7fb,
			...baseMaterialProps
		})
	);

	/** Screen-pixel threshold for edge picking (how close the cursor must be to an
	 *  edge's projection). Converted to world units per frame — see worldPerPixel. */
	const EDGE_PICK_THRESHOLD_PX = 6;

	// Reusable raycaster for edge picking
	const _edgeRaycaster = new THREE.Raycaster();
	const _edgeMouse = new THREE.Vector2();

	/**
	 * Build line segments geometry from edge render data.
	 *
	 * Each edge range is a POLYLINE (a straight edge is 2 points, a circle rim
	 * N+1), but THREE.LineSegments consumes vertices in PAIRS. Drawing the raw
	 * buffer therefore skipped every other chord of a curved edge — the rim
	 * rendered as a dashed line. An index buffer expands each polyline into
	 * consecutive (k, k+1) pairs; positions keep their vertex indices, so edge
	 * picking (which maps the hit's vertex index to a range) is unchanged.
	 * Groups, one per edge range for per-edge materials, are in index space.
	 */
	function buildEdgeGeometry(edgeData) {
		if (!edgeData || !edgeData.vertices || edgeData.vertices.length === 0) return null;
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(edgeData.vertices, 3));

		const vertexCount = edgeData.vertices.length / 3;
		const ranges =
			edgeData.ranges && edgeData.ranges.length > 0
				? edgeData.ranges
				: [{ start_index: 0, end_index: vertexCount }];

		const indices = [];
		geo.clearGroups();
		for (let i = 0; i < ranges.length; i++) {
			const { start_index: si, end_index: ei } = ranges[i];
			const groupStart = indices.length;
			for (let k = si; k < ei - 1; k++) indices.push(k, k + 1);
			geo.addGroup(groupStart, indices.length - groupStart, i);
		}
		geo.setIndex(indices);

		return geo;
	}

	/**
	 * Build materials array for edge ranges based on hover/selection state.
	 */
	/** The plain-edge material array, reused across rebuilds (see below). */
	let plainEdgeCache = { key: null, arr: null };

	function buildEdgeMaterials(ranges, hoveredRef, selectedRefs) {
		if (!ranges || ranges.length === 0) {
			return [fallbackMaterial];
		}

		const make = (color) =>
			withEdgeDepthBias(new THREE.LineBasicMaterial({ color, ...baseMaterialProps }));
		// Every unhighlighted edge in the document is the same colour, so they
		// can be the same material — one per edge meant a material, a program
		// bind and a draw call for every edge of every body.
		// Cached ACROSS rebuilds so a hover hands every other body back the
		// identical array and Svelte updates one object, not two thousand.
		const key = DEFAULT_EDGE_COLOR.getHexString();
		const plainArray = () => {
			if (plainEdgeCache.key !== key) {
				plainEdgeCache = { key, arr: [make(DEFAULT_EDGE_COLOR)] };
			}
			return plainEdgeCache.arr;
		};
		const plain = () => plainArray()[0];

		const perEdge = ranges.map((range) => {
			const ref = range.geom_ref;
			if (selectedRefs.some((r) => geomRefEquals(r, ref))) return make(SELECTED_EDGE_COLOR);
			if (hoveredRef && geomRefEquals(hoveredRef, ref)) return make(HOVER_EDGE_COLOR);
			return plain();
		});

		// Uniform body ⇒ one material ⇒ one draw call for all its edges,
		// instead of one per geometry group
		// (docs/notes/eiffel/FEATURE_NOTES.md §10).
		if (perEdge.every((m) => m === plainEdgeCache.arr?.[0])) return plainArray();
		return perEdge.every((m) => m === perEdge[0]) ? [perEdge[0]] : perEdge;
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

		// Find which edge range owns this vertex index. The index is local to
		// the hit object's geometry, so only the body that object draws is
		// searched: with several bodies (every assembly), the first body whose
		// range bracket happened to contain the number used to win.
		const meshData = getMeshes();
		if (!meshData) return null;
		const hitBodyId = hit.object?.userData?.bodyId ?? null;

		for (const mesh of meshData) {
			if (hitBodyId != null && mesh.bodyId !== hitBodyId) continue;
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

	// Keep the depth-bias pixel scale in step with the canvas size.
	useTask(() => {
		const h = renderer?.domElement?.clientHeight;
		if (h) viewportHeightUniform.value = h;
	});

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
				bodyId: m.bodyId,
				// The owning instance's solved placement (identity in a Part):
				// the edges are in part space, exactly like the faces.
				...placementProps(m.transform)
			}))
			.filter((e) => e.geometry !== null);
	});

	// Publish the real rendered edge-body count for GUI test introspection.
	$effect(() => setRenderedEdgeBodyCount(edgeGeometries.length));

	// Build material arrays reactively based on hover/selection state
	let edgeMaterials = $derived.by(() => {
		const hRef = getHoveredRef();
		const sRefs = getSelectedRefs();
		// Only bodies that could hold a lit edge get their ranges walked; every
		// other one takes the shared plain array. Without this a pointer move
		// compared every edge of every body by canonical JSON.
		const refFeature = (r) => r?.anchor?.feature_id ?? null;
		const lit = new Set();
		if (refFeature(hRef)) lit.add(refFeature(hRef));
		for (const r of sRefs) if (refFeature(r)) lit.add(refFeature(r));
		return edgeGeometries.map((e) =>
			lit.has(e.featureId)
				? buildEdgeMaterials(e.ranges, hRef, sRefs)
				: buildEdgeMaterials(e.ranges, null, [])
		);
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

	// The ranged materials are rebuilt from DEFAULT_EDGE_COLOR whenever it
	// changes; the fallback singleton is not, so re-tint it in place.
	$effect(() => {
		fallbackMaterial.color.copy(DEFAULT_EDGE_COLOR);
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
		position={edge.position}
		rotation={edge.rotation}
		userData={{ waffleType: 'edges', bodyId: edge.bodyId }}
		renderOrder={1}
	/>
{/each}
