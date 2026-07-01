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
		getHoveredBodyId,
		getSelectedFeatureId,
		getSectionState,
		isBodyVisible
	} from '$lib/engine/store.svelte.js';
	import { SIDE_FACE_GROUP_THRESHOLD } from '$lib/config.js';
	import { buildSectionClipPlane } from './sectionPlane.js';

	const DEFAULT_COLOR = new THREE.Color(0x8899aa);
	const HOVER_COLOR = new THREE.Color(0xaabbdd);
	const SELECTED_COLOR = new THREE.Color(0x44aaff);
	const PICK_HOVER_COLOR = new THREE.Color(0x55cc88);
	const BODY_SELECTED_COLOR = new THREE.Color(0x44aaff);
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
	function buildMaterials(faceRanges, hoveredRef, selectedRefs, inSketchMode, selectedFeatureId) {
		const projectActive = isProjectToolActive();
		const transparent = inSketchMode && !projectActive;
		const opacity = transparent ? 0.2 : (projectActive ? 0.5 : 1.0);

		const makeMat = (color) => {
			const mat = new THREE.MeshStandardMaterial({
				color,
				metalness: 0.3,
				roughness: 0.6,
				transparent,
				opacity,
				depthWrite: !transparent,
				side: THREE.DoubleSide
			});
			mat.onBeforeCompile = (shader) => {
				if (window.__waffle?.shaderDebug) {
					console.log('Vertex shader:', shader.vertexShader.substring(0, 500));
					console.log('Fragment shader:', shader.fragmentShader.substring(0, 500));
				}
			};
			return mat;
		};

		if (!faceRanges || faceRanges.length === 0) {
			return [makeMat(DEFAULT_COLOR)];
		}

		const groupSideFaces = shouldGroupSideFaces(faceRanges);
		const compareFn = (a, b) => {
			if (groupSideFaces && isGroupableSideFace(a) && isGroupableSideFace(b)) {
				return geomRefSameRoleType(a, b);
			}
			return geomRefEquals(a, b);
		};

		// Create shared materials — reuse instances for groups with the same visual state
		const defaultMat = makeMat(DEFAULT_COLOR);
		const pickMode = getProfilePickMode()?.target === 'extrude';
		let hoverMat = null;
		let selectedMat = null;
		let featureMat = null;

		return faceRanges.map((range) => {
			const ref = range.geom_ref;

			if (pickMode) {
				if (hoveredRef && compareFn(hoveredRef, ref)) {
					if (!hoverMat) hoverMat = makeMat(PICK_HOVER_COLOR);
					return hoverMat;
				}
			} else if (!inSketchMode) {
				if (selectedRefs.some((r) => compareFn(r, ref))) {
					if (!selectedMat) selectedMat = makeMat(SELECTED_COLOR);
					return selectedMat;
				}
				if (hoveredRef && compareFn(hoveredRef, ref)) {
					if (!hoverMat) hoverMat = makeMat(HOVER_COLOR);
					return hoverMat;
				}
				// KV13 F6c (inverse): a feature is selected in the tree → highlight
				// the faces it INTRODUCED (lowest precedence — explicit face
				// selection/hover above still win).
				if (selectedFeatureId && range.created_by_feature === selectedFeatureId) {
					if (!featureMat) featureMat = makeMat(FEATURE_FACE_COLOR);
					return featureMat;
				}
			}

			return defaultMat;
		});
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
			bodyId: m.bodyId
		}));
	});

	/**
	 * Build a single material that highlights an entire body. Returned as a
	 * one-element array so the template binds it as a single Material (three.js
	 * then applies it across all geometry groups, ignoring face ranges).
	 */
	function makeBodyMaterial(color) {
		return [
			new THREE.MeshStandardMaterial({
				color,
				metalness: 0.3,
				roughness: 0.6,
				side: THREE.DoubleSide
			})
		];
	}

	// Build material arrays reactively based on hover/selection/sketch-mode.
	// A whole-body selection/hover (from the Bodies list) overrides per-face
	// materials for the matching mesh, but never while in sketch mode.
	let meshMaterials = $derived.by(() => {
		const hRef = getHoveredRef();
		const sRefs = getSelectedRefs();
		const inSketch = getSketchMode()?.active ?? false;
		const selectedBody = getSelectedBodyId();
		const hoveredBody = getHoveredBodyId();
		const selFeature = getSelectedFeatureId();
		return engineMeshes.map((m) => {
			if (!inSketch && m.bodyId) {
				if (m.bodyId === selectedBody) return makeBodyMaterial(BODY_SELECTED_COLOR);
				if (m.bodyId === hoveredBody) return makeBodyMaterial(BODY_HOVER_COLOR);
			}
			return buildMaterials(m.faceRanges, hRef, sRefs, inSketch, selFeature);
		});
	});

	let showTestBox = $derived(engineMeshes.length === 0);
	let inSketchMode = $derived(getSketchMode()?.active ?? false);

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

		setHoveredRef(ref);
	}

	/**
	 * Handle pointer leaving mesh.
	 */
	function handlePointerOut() {
		setHoveredRef(null);
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
		const mesh = engineMeshes[meshIndex];
		if (!mesh || !mesh.faceRanges.length) return;

		// Extrude target-body pick mode: clicking a body toggles it in the dialog's
		// target set instead of selecting a face. Guarded — no effect when inactive.
		if (getExtrudeTargetPick().active && mesh.bodyId) {
			event.stopPropagation();
			toggleExtrudeTargetId(mesh.bodyId);
			return;
		}

		const faceIndex = event.faceIndex;
		if (faceIndex == null) return;

		const screenX = event.nativeEvent?.clientX ?? 0;
		const screenY = event.nativeEvent?.clientY ?? 0;
		const additive = event.nativeEvent?.shiftKey ?? false;

		let ref = findFaceRef(mesh.faceRanges, faceIndex);
		if (!ref) return;

		// Stop event from reaching datum planes behind this mesh
		event.stopPropagation();

		// Canonicalize SideFace refs when grouping
		if (shouldGroupSideFaces(mesh.faceRanges)) {
			ref = canonicalizeSideFaceRef(ref, mesh.faceRanges);
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
		{#if inSketchMode && !isProjectToolActive()}
			<T.Mesh
				geometry={mesh.geometry}
				material={meshMaterials[i]?.length > 1 ? meshMaterials[i] : meshMaterials[i]?.[0]}
				frustumCulled={false}
				userData={{ waffleType: 'model' }}
				raycast={() => {}}
			/>
		{:else}
			<T.Mesh
				geometry={mesh.geometry}
				material={meshMaterials[i]?.length > 1 ? meshMaterials[i] : meshMaterials[i]?.[0]}
				frustumCulled={false}
				userData={{ waffleType: 'model' }}
				onpointermove={(e) => handlePointerMove(e, i)}
				onpointerout={handlePointerOut}
				onclick={(e) => handleClick(e, i)}
				onpointermissed={handleMiss}
			/>
		{/if}
	{/each}
{/if}
