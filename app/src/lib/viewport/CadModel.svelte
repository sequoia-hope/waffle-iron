<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import {
		getMeshes,
		setHoveredRef,
		selectRef,
		clearSelection,
		getHoveredRef,
		getSelectedRefs,
		getSketchMode,
		geomRefEquals,
		geomRefSameRoleType,
		isSelected,
		getSelectOtherState,
		setSelectOtherState,
		getProfilePickMode,
		getExtrudeTargetPick,
		toggleExtrudeTargetId,
		isProjectToolActive,
		getSelectedBodyId,
		getGhostFeatureId,
		getHoveredBodyId,
		getSelectedFeatureId,
		getSectionState,
		isBodyVisible,
		isBodyPickingEnabled,
		proposeHoverRef,
		retractFaceHover,
		getSketchHover,
		getFreshHoveredRef,
		setSelectedInstancePath,
		getFeatureTree,
		getMaterials
	} from '$lib/engine/store.svelte.js';
	import { proposeEntityCard, openEntityDetail } from '$lib/engine/store.svelte.js';
	import { SIDE_FACE_GROUP_THRESHOLD } from '$lib/config.js';
	import { handleBodyFaceClick } from '$lib/sketch/tools.js';
	import { buildSectionClipPlane } from './sectionPlane.js';
	import { placementProps } from './placement.js';
	import { getTheme } from '$lib/ui/theme.svelte.js';
	import { getColorVersion, getSettings } from '$lib/ui/settings.svelte.js';
	import { resolveStudioFinish } from './materialPresets.js';
	import {
		createStudioPhysicalMaterial,
		attachFdmShaderToStandardMaterial
	} from './studio/studioMaterial.js';

	/**
	 * Resolve a CSS custom property on <html> to a THREE.Color. Falls back to
	 * `fallbackHex` if the var is unset or we're off-DOM (SSR).
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

	// Base face color is theme-driven (see --model-color in app.css) and
	// customizable from Settings -> Appearance. Reading getTheme() and
	// getColorVersion() makes this recompute on a theme switch or a per-token
	// override; every material builder below reads DEFAULT_COLOR, so their
	// reactive derives rebuild too.
	let DEFAULT_COLOR = $derived.by(() => {
		void getTheme(); void getColorVersion();
		return cssColor('--model-color', 0x8899aa);
	});
	const HOVER_COLOR = new THREE.Color(0xaabbdd);
	const SELECTED_COLOR = new THREE.Color(0x44aaff);
	// Ghost bodies of an edit context (other instances of the assembly the
	// open part is edited in): translucent and tinted, faces still pickable.
	const CONTEXT_GHOST_COLOR = new THREE.Color(0x9fb8a8);
	const PICK_HOVER_COLOR = new THREE.Color(0x55cc88);
	const BODY_SELECTED_COLOR = new THREE.Color(0x44aaff);
	const GHOST_PREVIEW_COLOR = new THREE.Color(0x4aa3ff);
	const BODY_HOVER_COLOR = new THREE.Color(0x6fc0ff);
	// KV13 F6c: faces whose CREATING feature is the one selected in the tree
	// (the inverse of click-face→feature). Green, matching the tree's
	// face-source accent.
	const FEATURE_FACE_COLOR = new THREE.Color(0x5fcf8f);

	/**
	 * Check if a GeomRef is a SideFace role.
	 * @param {any} ref
	 * @returns {boolean}
	 */
	function isGroupableSideFace(ref) {
		return ref?.selector?.type === 'Role' && ref?.selector?.role?.type === 'SideFace';
	}

	/**
	 * Check if SideFace grouping should be active for a set of face ranges.
	 * Returns true when there are many SideFace ranges (polygon approximation of a curve).
	 * @param {Array<{geom_ref: any}>} faceRanges
	 * @returns {boolean}
	 */
	function shouldGroupSideFaces(faceRanges) {
		if (!faceRanges) return false;
		let sideFaceCount = 0;
		for (const range of faceRanges) {
			if (isGroupableSideFace(range.geom_ref)) {
				sideFaceCount++;
			}
		}
		return sideFaceCount > SIDE_FACE_GROUP_THRESHOLD;
	}

	/**
	 * Canonicalize a SideFace ref to the first SideFace in the face ranges.
	 * This allows all SideFace facets to match the same canonical ref.
	 * @param {any} ref
	 * @param {Array<{geom_ref: any}>} faceRanges
	 * @returns {any}
	 */
	function canonicalizeSideFaceRef(ref, faceRanges) {
		if (!isGroupableSideFace(ref)) return ref;
		for (const range of faceRanges) {
			if (isGroupableSideFace(range.geom_ref)) {
				return range.geom_ref;
			}
		}
		return ref;
	}

	/**
	 * Binary search face_ranges to find the GeomRef owning a triangle index.
	 * face_ranges are sorted by start_index.
	 * @param {Array<{geom_ref: any, start_index: number, end_index: number}>} faceRanges
	 * @param {number} triangleIndex - index into the indices array (triangle * 3)
	 * @returns {any | null} GeomRef or null
	 */
	function findFaceRef(faceRanges, triangleIndex) {
		if (!faceRanges || faceRanges.length === 0) return null;
		const indexIntoIndices = triangleIndex * 3;
		let lo = 0;
		let hi = faceRanges.length - 1;
		while (lo <= hi) {
			const mid = (lo + hi) >> 1;
			const range = faceRanges[mid];
			if (indexIntoIndices < range.start_index) {
				hi = mid - 1;
			} else if (indexIntoIndices >= range.end_index) {
				lo = mid + 1;
			} else {
				return range.geom_ref;
			}
		}
		return null;
	}

	/**
	 * Build a BufferGeometry from engine mesh data with face-range groups.
	 * Groups allow per-face material assignment for hover/selection highlighting.
	 */
	function buildGeometry(meshData) {
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(meshData.vertices, 3));
		if (meshData.normals && meshData.normals.length > 0) {
			geo.setAttribute('normal', new THREE.BufferAttribute(meshData.normals, 3));
		}
		if (meshData.indices && meshData.indices.length > 0) {
			geo.setIndex(new THREE.BufferAttribute(meshData.indices, 1));
		}
		if (!meshData.normals || meshData.normals.length === 0) {
			geo.computeVertexNormals();
		}

		// Add groups for face ranges (enables per-face materials).
		// Three.js only renders indices belonging to a group, so any gap in
		// coverage would cause invisible triangles.
		if (meshData.faceRanges && meshData.faceRanges.length > 0) {
			geo.clearGroups();
			const totalIndices = meshData.indices ? meshData.indices.length : 0;
			let maxCovered = 0;
			for (let i = 0; i < meshData.faceRanges.length; i++) {
				const range = meshData.faceRanges[i];
				geo.addGroup(range.start_index, range.end_index - range.start_index, i);
				if (range.end_index > maxCovered) maxCovered = range.end_index;
			}
			// Catch-all for any indices not covered by face ranges
			if (maxCovered < totalIndices) {
				geo.addGroup(maxCovered, totalIndices - maxCovered, 0);
			}
		}

		return geo;
	}

	/**
	 * Build materials array for face ranges based on hover/selection/sketch-mode state.
	 * Uses shared material instances to avoid creating thousands of materials for
	 * complex geometry (e.g., gear profiles with 1600+ face ranges).
	 */
	/**
	 * The feature a GeomRef points into, or null. Used to decide, in O(1),
	 * whether a body can possibly hold the highlighted face.
	 * @param {any} ref
	 */
	function refFeatureId(ref) {
		return ref?.anchor?.feature_id ?? null;
	}

	/**
	 * Invariant I3 (`specs/studio_pbr_renderer.md`): bounded plain-body material
	 * cache keyed by `(shadingMode, color, metalness, roughness, finish, transparent, opacity)`
	 * so unhighlighted bodies sharing a material share one `[Material]` array.
	 * @type {Map<string, THREE.Material[]>}
	 */
	const plainCacheMap = new Map();
	const MAX_PLAIN_CACHE = 64;

	/**
	 * Resolve a body's assigned material name and Appearance from its render-view
	 * metadata (authoritative for Part, Assembly, and `/view` host modes) with
	 * live fallback to the active part's `FeatureTree`.
	 */
	function resolveBodyMaterialSpec(mesh, treeMaterials, bodyMaterialsMap) {
		const matName =
			mesh?.material ??
			(mesh?.bodyId && bodyMaterialsMap ? bodyMaterialsMap[mesh.bodyId] ?? null : null);
		let appearance = mesh?.appearance ?? null;
		if (!appearance && matName && Array.isArray(treeMaterials)) {
			const found = treeMaterials.find((m) => m.name === matName);
			if (found?.appearance) appearance = found.appearance;
		}
		return { matName, appearance };
	}

	/**
	 * Build a single Three.js material for the active shading mode (`shaded` vs `studio`).
	 * Invariant I1: when `shadingMode === 'shaded'` and `spec` is null, produces the
	 * exact classic `MeshStandardMaterial` (`metalness: 0.3, roughness: 0.6`).
	 */
	function createCadSurfaceMaterial({
		shadingMode,
		color,
		metalness,
		roughness,
		finish,
		transparent,
		opacity
	}) {
		if (shadingMode === 'studio') {
			return createStudioPhysicalMaterial({
				color,
				metalness,
				roughness,
				transparent,
				opacity,
				finish
			});
		}
		const mat = new THREE.MeshStandardMaterial({
			color,
			metalness,
			roughness,
			transparent,
			opacity,
			depthWrite: !transparent,
			side: THREE.DoubleSide
		});
		if (finish?.printLayerMm > 0) {
			attachFdmShaderToStandardMaterial(mat, finish.printLayerMm);
			return mat;
		}
		mat.onBeforeCompile = (shader) => {
			if (window.__waffle?.shaderDebug) {
				console.log('Vertex shader:', shader.vertexShader.substring(0, 500));
				console.log('Fragment shader:', shader.fragmentShader.substring(0, 500));
			}
		};
		return mat;
	}

	function resolveBaseSurfaceProps(shadingMode, ghost, matSpec, globalPrintMm = 0) {
		if (ghost) {
			return {
				color: CONTEXT_GHOST_COLOR,
				metalness: shadingMode === 'studio' ? 0.15 : 0.3,
				roughness: shadingMode === 'studio' ? 0.5 : 0.6,
				finish: null
			};
		}
		const app = matSpec?.appearance;
		if (app && Array.isArray(app.color) && app.color.length >= 3) {
			const c = new THREE.Color().setRGB(
				app.color[0],
				app.color[1],
				app.color[2],
				THREE.SRGBColorSpace
			);
			const rawMetal = typeof app.metalness === 'number' ? app.metalness : (shadingMode === 'studio' ? 0.15 : 0.3);
			const rawRough = typeof app.roughness === 'number' ? app.roughness : (shadingMode === 'studio' ? 0.42 : 0.6);
			const presetFinish = resolveStudioFinish(matSpec?.matName);
			const printLayerMm = globalPrintMm > 0 ? globalPrintMm : (presetFinish?.printLayerMm ?? 0);
			const finish =
				shadingMode === 'studio'
					? presetFinish || printLayerMm > 0
						? { ...(presetFinish ?? {}), printLayerMm }
						: null
					: printLayerMm > 0
						? { printLayerMm }
						: null;
			return {
				color: c,
				// Without an IBL map in classic Shaded mode, clamp metalness <= 0.45 so metals stay legible
				metalness: shadingMode === 'studio' ? rawMetal : Math.min(rawMetal, 0.45),
				roughness: rawRough,
				finish
			};
		}
		// Unassigned body: exact classic parameters in Shaded mode when globalPrintMm == 0 (Invariant I1)
		if (shadingMode === 'studio') {
			return {
				color: DEFAULT_COLOR,
				metalness: 0.18,
				roughness: 0.38,
				finish: {
					clearcoat: 0.08,
					clearcoatRoughness: 0.28,
					anisotropy: 0.0,
					anisotropyRotation: 0.0,
					printLayerMm: globalPrintMm
				}
			};
		}
		return {
			color: DEFAULT_COLOR,
			metalness: 0.3,
			roughness: 0.6,
			finish: globalPrintMm > 0 ? { printLayerMm: globalPrintMm } : null
		};
	}

	function plainMaterials(inSketchMode, shadingMode, matSpec, globalPrintMm = 0) {
		return buildMaterials(null, null, [], inSketchMode, null, false, shadingMode, matSpec, globalPrintMm);
	}

	function buildMaterials(
		faceRanges,
		hoveredRef,
		selectedRefs,
		inSketchMode,
		selectedFeatureId,
		ghost = false,
		shadingMode = 'shaded',
		matSpec = null,
		globalPrintMm = 0
	) {
		const projectActive = isProjectToolActive();
		const baseProps = resolveBaseSurfaceProps(shadingMode, ghost, matSpec, globalPrintMm);
		const finishOpacity =
			!ghost && !inSketchMode && !projectActive && typeof baseProps.finish?.opacity === 'number'
				? baseProps.finish.opacity
				: 1.0;
		const modeTransparent = ghost || (inSketchMode && !projectActive);
		const transparent = modeTransparent || finishOpacity < 0.999;
		const opacity = ghost
			? (inSketchMode && !projectActive ? 0.2 : 0.4)
			: (modeTransparent ? 0.2 : (projectActive ? 0.5 : finishOpacity));

		const makeDefaultMat = () =>
			createCadSurfaceMaterial({
				shadingMode,
				color: baseProps.color,
				metalness: baseProps.metalness,
				roughness: baseProps.roughness,
				finish: baseProps.finish,
				transparent,
				opacity
			});

		const makeHighlightMat = (color) =>
			createCadSurfaceMaterial({
				shadingMode,
				color,
				metalness: shadingMode === 'studio' ? 0.2 : 0.3,
				roughness: shadingMode === 'studio' ? 0.45 : 0.6,
				finish: null,
				transparent: modeTransparent,
				opacity: modeTransparent ? opacity : (projectActive ? 0.5 : 1.0)
			});

		const finishKey = baseProps.finish
			? `${baseProps.finish.clearcoat ?? 0}:${baseProps.finish.anisotropy ?? 0}:${baseProps.finish.ior ?? 1.5}:${baseProps.finish.printLayerMm ?? 0}`
			: 'none';
		const plainKey = `${shadingMode}|${baseProps.color.getHexString()}|${baseProps.metalness.toFixed(4)}|${baseProps.roughness.toFixed(4)}|${finishKey}|${transparent}|${opacity}`;
		const plainArray = () => {
			let hit = plainCacheMap.get(plainKey);
			if (!hit) {
				if (plainCacheMap.size >= MAX_PLAIN_CACHE) {
					const oldestKey = plainCacheMap.keys().next().value;
					const evicted = plainCacheMap.get(oldestKey);
					if (evicted) evicted.forEach((m) => m.dispose());
					plainCacheMap.delete(oldestKey);
				}
				hit = [makeDefaultMat()];
				plainCacheMap.set(plainKey, hit);
			}
			return hit;
		};
		const plainMat = () => plainArray()[0];

		if (!faceRanges || faceRanges.length === 0) {
			return plainArray();
		}

		const groupSideFaces = shouldGroupSideFaces(faceRanges);
		const compareFn = (a, b) => {
			if (groupSideFaces && isGroupableSideFace(a) && isGroupableSideFace(b)) {
				return geomRefSameRoleType(a, b);
			}
			return geomRefEquals(a, b);
		};

		// Create shared materials — reuse instances for groups with the same visual state
		const defaultMat = plainMat();
		const pickMode = getProfilePickMode()?.target === 'extrude';
		let hoverMat = null;
		let selectedMat = null;
		let featureMat = null;

		const perFace = faceRanges.map((range) => {
			const ref = range.geom_ref;

			if (pickMode) {
				if (hoveredRef && compareFn(hoveredRef, ref)) {
					if (!hoverMat) hoverMat = makeHighlightMat(PICK_HOVER_COLOR);
					return hoverMat;
				}
			} else if (!inSketchMode) {
				if (selectedRefs.some((r) => compareFn(r, ref))) {
					if (!selectedMat) selectedMat = makeHighlightMat(SELECTED_COLOR);
					return selectedMat;
				}
				if (hoveredRef && compareFn(hoveredRef, ref)) {
					if (!hoverMat) hoverMat = makeHighlightMat(HOVER_COLOR);
					return hoverMat;
				}
				// KV13 F6c (inverse): a feature is selected in the tree → highlight
				// the faces it INTRODUCED (lowest precedence — explicit face
				// selection/hover above still win).
				if (selectedFeatureId && range.created_by_feature === selectedFeatureId) {
					if (!featureMat) featureMat = makeHighlightMat(FEATURE_FACE_COLOR);
					return featureMat;
				}
			}

			return defaultMat;
		});

		// Nothing on this body is highlighted: collapse to a single material draw call.
		if (perFace.every((m) => m === defaultMat)) return plainArray();
		return perFace.every((m) => m === perFace[0]) ? [perFace[0]] : perFace;
	}

	// Create fallback test box geometry + material
	const testGeometry = new THREE.BoxGeometry(2, 2, 2);

	let testMaterial = $derived.by(() => {
		const inSketch = getSketchMode()?.active ?? false;
		return new THREE.MeshStandardMaterial({
			color: DEFAULT_COLOR,
			metalness: 0.3,
			roughness: 0.6,
			transparent: inSketch,
			opacity: inSketch ? 0.2 : 1.0,
			depthWrite: !inSketch,
			side: THREE.DoubleSide
		});
	});

	// Derive engine meshes with geometry objects
	let engineMeshes = $derived.by(() => {
		const meshData = getMeshes();
		if (!meshData || meshData.length === 0) return [];
		return meshData.filter((m) => isBodyVisible(m.bodyId)).map((m) => ({
			geometry: buildGeometry(m),
			faceRanges: m.faceRanges || [],
			featureId: m.featureId,
			bodyId: m.bodyId,
			material: m.material ?? null,
			appearance: m.appearance ?? null,
			instanceId: m.instanceId ?? null,
			instancePath: m.instancePath ?? null,
			context: m.context === true,
			...placementProps(m.transform)
		}));
	});

	/**
	 * Build a single material that highlights an entire body. Returned as a
	 * one-element array so the template binds it as a single Material (three.js
	 * then applies it across all geometry groups, ignoring face ranges).
	 */
	function makeBodyMaterial(color, shadingMode = 'shaded') {
		return [
			createCadSurfaceMaterial({
				shadingMode,
				color,
				metalness: shadingMode === 'studio' ? 0.2 : 0.3,
				roughness: shadingMode === 'studio' ? 0.45 : 0.6,
				finish: null,
				transparent: false,
				opacity: 1.0
			})
		];
	}

	/**
	 * Ghost-preview material for a body being placed by the STEP import
	 * modal: translucent so the user sees it against existing geometry.
	 */
	function makeGhostMaterial() {
		return [
			new THREE.MeshStandardMaterial({
				color: GHOST_PREVIEW_COLOR,
				metalness: 0.2,
				roughness: 0.5,
				transparent: true,
				opacity: 0.45,
				depthWrite: false,
				side: THREE.DoubleSide
			})
		];
	}

	// Build material arrays reactively based on hover/selection/sketch-mode/shading/appearance.
	// A whole-body selection/hover (from the Bodies list) overrides per-face
	// materials for the matching mesh, but never while in sketch mode.
	let meshMaterials = $derived.by(() => {
		const hRef = getHoveredRef();
		const sRefs = getSelectedRefs();
		const inSketch = getSketchMode()?.active ?? false;
		const selectedBody = getSelectedBodyId();
		const hoveredBody = getHoveredBodyId();
		const selFeature = getSelectedFeatureId();
		const ghostFeature = getGhostFeatureId();
		const cfgSettings = getSettings();
		const shadingMode = cfgSettings.viewportShading;
		const globalPrintMm = cfgSettings.studioPrintLayers ? (cfgSettings.studioLayerHeightMm ?? 0.24) : 0;
		const tree = getFeatureTree();
		const treeMaterials = getMaterials();
		const bodyMaterialsMap = tree?.body_materials ?? null;

		const litFeatures = new Set();
		if (refFeatureId(hRef)) litFeatures.add(refFeatureId(hRef));
		for (const r of sRefs) if (refFeatureId(r)) litFeatures.add(refFeatureId(r));
		const canSkip = !selFeature && !isProjectToolActive() && !getProfilePickMode();
		return engineMeshes.map((m) => {
			if (ghostFeature && m.featureId === ghostFeature) return makeGhostMaterial();
			if (!inSketch && m.bodyId && !m.context) {
				if (m.bodyId === selectedBody) return makeBodyMaterial(BODY_SELECTED_COLOR, shadingMode);
				if (m.bodyId === hoveredBody) return makeBodyMaterial(BODY_HOVER_COLOR, shadingMode);
			}
			const matSpec = resolveBodyMaterialSpec(m, treeMaterials, bodyMaterialsMap);
			if (canSkip && !m.context && !litFeatures.has(m.featureId)) {
				return plainMaterials(inSketch, shadingMode, matSpec, globalPrintMm);
			}
			return buildMaterials(
				m.faceRanges,
				hRef,
				sRefs,
				inSketch,
				selFeature,
				m.context,
				shadingMode,
				matSpec,
				globalPrintMm
			);
		});
	});

	let showTestBox = $derived(engineMeshes.length === 0);

	// Capped section view: derive the clip plane from section state and apply it
	// to the solid body materials only. Cleared (set to []) when inactive so the
	// normal view is restored exactly. Re-runs whenever the materials are rebuilt
	// (hover/selection) or the section plane changes.
	let sectionClipPlane = $derived.by(() => {
		const s = getSectionState();
		if (!s.active || !s.plane) return null;
		return buildSectionClipPlane(s.plane, s.flipped, s.offset);
	});

	$effect(() => {
		const plane = sectionClipPlane;
		const mats = meshMaterials;
		const planes = plane ? [plane] : [];
		for (const matArr of mats) {
			if (!matArr) continue;
			for (const mat of matArr) {
				if (!mat) continue;
				mat.clippingPlanes = planes;
				mat.clipShadows = false;
				mat.needsUpdate = true;
			}
		}
		// Also clip the fallback test material, harmless when no body present.
		if (testMaterial) {
			testMaterial.clippingPlanes = planes;
			testMaterial.needsUpdate = true;
		}
	});

	/**
	 * Handle pointer move on mesh for hover highlighting.
	 */
	function handlePointerMove(event, meshIndex) {
		// Invariant I1: a sketch entity under the pointer wins over the body behind
		// it — never hover the body when a sketch entity is hovered.
		if (getSketchMode()?.active && getSketchHover() != null) return;
		const mesh = engineMeshes[meshIndex];
		if (!mesh || !mesh.faceRanges.length) return;
		const faceIndex = event.faceIndex;
		if (faceIndex == null) return;
		let ref = findFaceRef(mesh.faceRanges, faceIndex);
		if (!ref) return;

		// Stop event from reaching datum planes behind this mesh
		event.stopPropagation();

		// Canonicalize SideFace refs when grouping so all facets highlight together
		if (shouldGroupSideFaces(mesh.faceRanges)) {
			ref = canonicalizeSideFaceRef(ref, mesh.faceRanges);
		}

		// Invariant I3: Face is the lowest hover priority — propose it for this
		// pixel; a Vertex/Edge proposal for the same pixel supersedes it.
		const ne = event.nativeEvent;
		lastFaceHoverPx = { x: ne?.clientX ?? 0, y: ne?.clientY ?? 0 };
		proposeHoverRef(ref, lastFaceHoverPx.x, lastFaceHoverPx.y);
		// Board data on hover (specs/kicad_board_link.md C4): what a linked
		// KiCad board knows about this body, if anything.
		proposeEntityCard(
			mesh.bodyId,
			mesh.instancePath ?? (mesh.instanceId ? [mesh.instanceId] : null),
			ne?.clientX ?? 0,
			ne?.clientY ?? 0
		);
	}

	/**
	 * The pixel of the last Face hover a mesh proposed.
	 * @type {{ x: number, y: number } | null}
	 */
	let lastFaceHoverPx = null;

	/**
	 * Handle pointer leaving mesh: withdraw the Face hover, unless a newer
	 * pixel has been arbitrated since it was proposed (see `retractFaceHover`).
	 */
	function handlePointerOut() {
		retractFaceHover(lastFaceHoverPx?.x ?? null, lastFaceHoverPx?.y ?? null);
		lastFaceHoverPx = null;
	}

	/** Threshold in pixels for "same click position" detection */
	const SAME_POS_THRESHOLD = 5;

	/**
	 * Collect all face GeomRefs under the click point across all meshes.
	 * Uses THREE.Raycaster to get ALL intersections sorted by distance.
	 * @param {any} event - Threlte pointer event
	 * @returns {Array<any>} Array of GeomRefs sorted front-to-back
	 */
	function collectAllRefsAtPoint(event) {
		if (!event.nativeEvent) return [];

		const refs = [];
		const seen = new Set();

		// Use event.intersections if available (Threlte provides sorted intersections)
		// Otherwise fall back to the single faceIndex
		for (const mesh of engineMeshes) {
			if (!mesh.faceRanges.length) continue;

			// Check all face ranges — if the event has intersections, use faceIndex
			// For Select Other, we rely on the primary click's faceIndex
		}

		// Collect from primary hit first
		for (let mi = 0; mi < engineMeshes.length; mi++) {
			const mesh = engineMeshes[mi];
			if (!mesh.faceRanges.length) continue;

			// The event gives us the faceIndex for this specific mesh
			if (mi === getCurrentMeshIndex(event)) {
				const faceIndex = event.faceIndex;
				if (faceIndex != null) {
					const ref = findFaceRef(mesh.faceRanges, faceIndex);
					if (ref) {
						const key = JSON.stringify(ref);
						if (!seen.has(key)) {
							seen.add(key);
							refs.push(ref);
						}
					}
				}
			}

			// Also add all unique face refs for this mesh (for cycling)
			for (const range of mesh.faceRanges) {
				if (range.geom_ref) {
					const key = JSON.stringify(range.geom_ref);
					if (!seen.has(key)) {
						seen.add(key);
						refs.push(range.geom_ref);
					}
				}
			}
		}

		return refs;
	}

	/**
	 * Get the mesh index from an event (stored during handler dispatch).
	 * @param {any} _event
	 * @returns {number}
	 */
	function getCurrentMeshIndex(_event) {
		return _event._meshIndex ?? 0;
	}

	/**
	 * Handle click on mesh for selection with Select Other cycling.
	 * If user clicks at approximately the same screen position as last click,
	 * advance cycle index to select the next face behind.
	 */
	function handleClick(event, meshIndex) {
		// Invariant I1: a sketch entity under the pointer wins — never select the
		// body face behind a hovered sketch entity. In sketch mode CadModel owns
		// FACE selection only (Vertex/Edge are selected by the sketch pointerdown
		// path off their synchronous DOM-listener hover, which is ready at
		// pointerdown; the Threlte face hover only settles by click time). The
		// defer-to-hovered-Vertex/Edge below prevents any double-handling.
		if (getSketchMode()?.active && getSketchHover() != null) return;
		const mesh = engineMeshes[meshIndex];
		if (!mesh || !mesh.faceRanges.length) return;

		// Extrude target-body pick mode: clicking a body toggles it in the dialog's
		// target set instead of selecting a face. Guarded — no effect when inactive.
		if (getExtrudeTargetPick().active && mesh.bodyId) {
			event.stopPropagation();
			toggleExtrudeTargetId(mesh.bodyId);
			return;
		}

		// Invariant I3: Face is the lowest priority — defer selection to a hovered
		// Vertex/Edge (the overlays select those). Face still wins when nothing
		// higher-priority is hovered (preserving Select-Other face cycling). In
		// sketch mode use the pixel-fresh hover so a STALE Vertex/Edge (carried from
		// a previous pixel, before the frame-deferred face hover refreshes) does not
		// wrongly suppress a genuine face click.
		const hov = getSketchMode()?.active ? getFreshHoveredRef() : getHoveredRef();
		if (hov?.kind?.type === 'Vertex' || hov?.kind?.type === 'Edge') return;

		const faceIndex = event.faceIndex;
		if (faceIndex == null) return;

		const screenX = event.nativeEvent?.clientX ?? 0;
		const screenY = event.nativeEvent?.clientY ?? 0;
		const additive = event.nativeEvent?.shiftKey ?? false;

		let ref = findFaceRef(mesh.faceRanges, faceIndex);
		if (!ref) return;

		// Stop event from reaching datum planes behind this mesh
		event.stopPropagation();

		// Assembly mode: remember which instance the clicked body belongs to
		// (mate connectors are created "on the selected face of an instance").
		setSelectedInstancePath(mesh.instancePath ?? (mesh.instanceId ? [mesh.instanceId] : null));
		// Board data on click: the detail panel for a body a KiCad board derived.
		openEntityDetail(mesh.bodyId, mesh.instancePath ?? (mesh.instanceId ? [mesh.instanceId] : null));

		// Canonicalize SideFace refs when grouping
		if (shouldGroupSideFaces(mesh.faceRanges)) {
			ref = canonicalizeSideFaceRef(ref, mesh.faceRanges);
		}

		// Tool-first Project/Offset on faces: the face ref only resolves
		// reliably HERE (Threlte raycast at click time — the tools'
		// window-pointerdown handlers see a stale/absent face hover), so the
		// sketch tools delegate face clicks to this path. Consumes the click.
		if (getSketchMode()?.active && handleBodyFaceClick(ref)) {
			return;
		}

		// In a sketch a repeat click on a picked face drops it (picks
		// accumulate), so Select Other cycling is a modeling-mode gesture.
		if (getSketchMode()?.active) {
			selectRef(ref, true);
			return;
		}

		// Check if this is a "same position" click for Select Other cycling
		const soState = getSelectOtherState();
		const dx = screenX - soState.lastScreenX;
		const dy = screenY - soState.lastScreenY;
		const samePosition = Math.sqrt(dx * dx + dy * dy) < SAME_POS_THRESHOLD;

		if (samePosition && soState.intersections.length > 1) {
			// Cycle to next ref in the intersection list
			const nextIndex = (soState.cycleIndex + 1) % soState.intersections.length;
			const nextRef = soState.intersections[nextIndex];
			setSelectOtherState({
				cycleIndex: nextIndex,
				lastScreenX: screenX,
				lastScreenY: screenY
			});
			selectRef(nextRef, additive);
		} else {
			// New position — build intersection list and select first
			// Collect all unique face refs from this mesh for cycling
			const allRefs = [];
			const seen = new Set();
			// Put the clicked ref first
			allRefs.push(ref);
			seen.add(JSON.stringify(ref));

			for (const range of mesh.faceRanges) {
				if (range.geom_ref) {
					const key = JSON.stringify(range.geom_ref);
					if (!seen.has(key)) {
						seen.add(key);
						allRefs.push(range.geom_ref);
					}
				}
			}

			setSelectOtherState({
				intersections: allRefs,
				cycleIndex: 0,
				lastScreenX: screenX,
				lastScreenY: screenY
			});
			selectRef(ref, additive);
		}
	}

	/**
	 * Handle click on empty space (background miss).
	 */
	function handleMiss() {
		clearSelection();
		setHoveredRef(null);
		setSelectOtherState({ intersections: [], cycleIndex: 0, lastScreenX: -1, lastScreenY: -1 });
	}
</script>

{#if !showTestBox}
	{#each engineMeshes as mesh, i (mesh.bodyId)}
		{#if !isBodyPickingEnabled()}
			<T.Mesh
				geometry={mesh.geometry}
				material={meshMaterials[i]?.length > 1 ? meshMaterials[i] : meshMaterials[i]?.[0]}
				position={mesh.position}
				rotation={mesh.rotation}
				frustumCulled={false}
				userData={{ waffleType: 'model', bodyId: mesh.bodyId }}
				raycast={() => {}}
			/>
		{:else}
			<T.Mesh
				geometry={mesh.geometry}
				material={meshMaterials[i]?.length > 1 ? meshMaterials[i] : meshMaterials[i]?.[0]}
				position={mesh.position}
				rotation={mesh.rotation}
				frustumCulled={false}
				userData={{ waffleType: 'model', bodyId: mesh.bodyId }}
				onpointermove={(e) => handlePointerMove(e, i)}
				onpointerout={handlePointerOut}
				onclick={(e) => handleClick(e, i)}
				onpointermissed={handleMiss}
			/>
		{/if}
	{/each}
{/if}
