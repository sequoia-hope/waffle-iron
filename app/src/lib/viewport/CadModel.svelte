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
		geomRefEquals,
		isSelected
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
	 * Build materials array for face ranges based on hover/selection state.
	 */
	function buildMaterials(faceRanges, hoveredRef, selectedRefs) {
		if (!faceRanges || faceRanges.length === 0) {
			return [
				new THREE.MeshStandardMaterial({
					color: DEFAULT_COLOR,
					metalness: 0.3,
					roughness: 0.6
				})
			];
		}

		return faceRanges.map((range) => {
			const ref = range.geom_ref;
			let color = DEFAULT_COLOR;

			if (selectedRefs.some((r) => geomRefEquals(r, ref))) {
				color = SELECTED_COLOR;
			} else if (hoveredRef && geomRefEquals(hoveredRef, ref)) {
				color = HOVER_COLOR;
			}

			return new THREE.MeshStandardMaterial({
				color,
				metalness: 0.3,
				roughness: 0.6
			});
		});
	}

	// Create fallback test box geometry + material
	const testGeometry = new THREE.BoxGeometry(2, 2, 2);
	const testMaterial = new THREE.MeshStandardMaterial({
		color: DEFAULT_COLOR,
		metalness: 0.3,
		roughness: 0.6
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

	// Build material arrays reactively based on hover/selection
	let meshMaterials = $derived.by(() => {
		const hRef = getHoveredRef();
		const sRefs = getSelectedRefs();
		return engineMeshes.map((m) => buildMaterials(m.faceRanges, hRef, sRefs));
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

	/**
	 * Handle click on mesh for selection.
	 */
	function handleClick(event, meshIndex) {
		const mesh = engineMeshes[meshIndex];
		if (!mesh || !mesh.faceRanges.length) return;
		const faceIndex = event.faceIndex;
		if (faceIndex == null) return;
		const ref = findFaceRef(mesh.faceRanges, faceIndex);
		if (ref) {
			const additive = event.nativeEvent?.shiftKey ?? false;
			selectRef(ref, additive);
		}
	}

	/**
	 * Handle click on empty space (background miss).
	 */
	function handleMiss() {
		clearSelection();
		setHoveredRef(null);
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
