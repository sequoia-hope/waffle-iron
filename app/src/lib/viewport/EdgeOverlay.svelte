<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getMeshes } from '$lib/engine/store.svelte.js';

	const edgeMaterial = new THREE.LineBasicMaterial({
		color: 0x222233,
		linewidth: 1,
		depthTest: true,
		polygonOffset: true,
		polygonOffsetFactor: -1,
		polygonOffsetUnits: -1
	});

	/**
	 * Build line segments geometry from edge render data.
	 * EdgeRenderData has flat vertex array: [x0,y0,z0, x1,y1,z1, ...] where each pair is a segment.
	 */
	function buildEdgeGeometry(edgeData) {
		if (!edgeData || !edgeData.vertices || edgeData.vertices.length === 0) return null;
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(edgeData.vertices, 3));
		return geo;
	}

	// Derive edge geometries from mesh state
	// The engine may provide edge data alongside meshes
	let edgeGeometries = $derived.by(() => {
		const meshData = getMeshes();
		if (!meshData) return [];
		return meshData
			.filter((m) => m.edges && m.edges.vertices && m.edges.vertices.length > 0)
			.map((m) => ({
				geometry: buildEdgeGeometry(m.edges),
				featureId: m.featureId
			}))
			.filter((e) => e.geometry !== null);
	});
</script>

{#each edgeGeometries as edge (edge.featureId)}
	<T.LineSegments geometry={edge.geometry} material={edgeMaterial} />
{/each}
