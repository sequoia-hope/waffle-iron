<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getMeshes } from '$lib/engine/store.svelte.js';

	// Create a hardcoded box for initial visual verification
	function createTestBox() {
		const geo = new THREE.BoxGeometry(2, 2, 2);
		return geo;
	}

	const testGeometry = createTestBox();
	const material = new THREE.MeshStandardMaterial({
		color: 0x8899aa,
		metalness: 0.3,
		roughness: 0.6,
	});

	/**
	 * Build a BufferGeometry from engine mesh data (TypedArrays).
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
		return geo;
	}

	// Derive engine geometries from reactive mesh state
	let engineMeshes = $derived.by(() => {
		const meshData = getMeshes();
		if (!meshData || meshData.length === 0) return [];
		return meshData.map((m) => ({
			geometry: buildGeometry(m),
			featureId: m.featureId,
		}));
	});

	// Show test box when no engine meshes available
	let showTestBox = $derived(engineMeshes.length === 0);
</script>

{#if showTestBox}
	<T.Mesh geometry={testGeometry} {material}>
		<T.MeshStandardMaterial color="#8899aa" metalness={0.3} roughness={0.6} />
	</T.Mesh>
{:else}
	{#each engineMeshes as mesh (mesh.featureId)}
		<T.Mesh geometry={mesh.geometry} {material} />
	{/each}
{/if}
