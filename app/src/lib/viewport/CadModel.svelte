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
		isSelected,
		getSelectOtherState,
		setSelectOtherState
	} from '$lib/engine/store.svelte.js';

	const DEFAULT_COLOR = new THREE.Color(0x8899aa);
	const HOVER_COLOR = new THREE.Color(0xaabbdd);
	const SELECTED_COLOR = new THREE.Color(0x44aaff);

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

		// Add groups for face ranges (enables per-face materials)
		if (meshData.faceRanges && meshData.faceRanges.length > 0) {
			geo.clearGroups();
			for (let i = 0; i < meshData.faceRanges.length; i++) {
				const range = meshData.faceRanges[i];
				geo.addGroup(range.start_index, range.end_index - range.start_index, i);
			}
		}

		return geo;
	}

	/**
	 * Build materials array for face ranges based on hover/selection/sketch-mode state.
	 */
	function buildMaterials(faceRanges, hoveredRef, selectedRefs, inSketchMode) {
		const transparent = inSketchMode;
		const opacity = transparent ? 0.2 : 1.0;

		if (!faceRanges || faceRanges.length === 0) {
			return [
				new THREE.MeshStandardMaterial({
					color: DEFAULT_COLOR,
					metalness: 0.3,
					roughness: 0.6,
					transparent,
					opacity,
					depthWrite: !transparent
				})
			];
		}

		return faceRanges.map((range) => {
			const ref = range.geom_ref;
			let color = DEFAULT_COLOR;

			if (!inSketchMode) {
				if (selectedRefs.some((r) => geomRefEquals(r, ref))) {
					color = SELECTED_COLOR;
				} else if (hoveredRef && geomRefEquals(hoveredRef, ref)) {
					color = HOVER_COLOR;
				}
			}

			return new THREE.MeshStandardMaterial({
				color,
				metalness: 0.3,
				roughness: 0.6,
				transparent,
				opacity,
				depthWrite: !transparent
			});
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
			depthWrite: !inSketch
		});
	});

	// Derive engine meshes with geometry objects
	let engineMeshes = $derived.by(() => {
		const meshData = getMeshes();
		if (!meshData || meshData.length === 0) return [];
		return meshData.map((m) => ({
			geometry: buildGeometry(m),
			faceRanges: m.faceRanges || [],
			featureId: m.featureId
		}));
	});

	// Build material arrays reactively based on hover/selection/sketch-mode
	let meshMaterials = $derived.by(() => {
		const hRef = getHoveredRef();
		const sRefs = getSelectedRefs();
		const inSketch = getSketchMode()?.active ?? false;
		return engineMeshes.map((m) => buildMaterials(m.faceRanges, hRef, sRefs, inSketch));
	});

	let showTestBox = $derived(engineMeshes.length === 0);

	/**
	 * Handle pointer move on mesh for hover highlighting.
	 */
	function handlePointerMove(event, meshIndex) {
		const mesh = engineMeshes[meshIndex];
		if (!mesh || !mesh.faceRanges.length) return;
		const faceIndex = event.faceIndex;
		if (faceIndex == null) return;
		const ref = findFaceRef(mesh.faceRanges, faceIndex);
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
		const faceIndex = event.faceIndex;
		if (faceIndex == null) return;

		const screenX = event.nativeEvent?.clientX ?? 0;
		const screenY = event.nativeEvent?.clientY ?? 0;
		const additive = event.nativeEvent?.shiftKey ?? false;

		const ref = findFaceRef(mesh.faceRanges, faceIndex);
		if (!ref) return;

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

{#if showTestBox}
	<T.Mesh geometry={testGeometry} material={testMaterial} />
{:else}
	{#each engineMeshes as mesh, i (mesh.featureId)}
		<T.Mesh
			geometry={mesh.geometry}
			material={meshMaterials[i]?.length > 1 ? meshMaterials[i] : meshMaterials[i]?.[0]}
			onpointermove={(e) => handlePointerMove(e, i)}
			onpointerout={handlePointerOut}
			onclick={(e) => handleClick(e, i)}
			onpointermissed={handleMiss}
		/>
	{/each}
{/if}
