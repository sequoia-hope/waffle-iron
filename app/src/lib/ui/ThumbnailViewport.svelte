<script>
	import { onMount } from 'svelte';
	import * as THREE from 'three';

	let { previewMesh = null, width = 240, height = 180 } = $props();

	let canvasEl;

	onMount(() => {
		if (!previewMesh || !canvasEl) return;

		const renderer = new THREE.WebGLRenderer({
			canvas: canvasEl,
			antialias: true,
			alpha: true
		});
		renderer.setSize(width, height);
		renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

		const scene = new THREE.Scene();
		scene.background = new THREE.Color(0x1e1e2e);

		// Build mesh from preview data
		const geometry = new THREE.BufferGeometry();
		const vertices = new Float32Array(previewMesh.vertices);
		const normals = new Float32Array(previewMesh.normals);
		const indices = new Uint32Array(previewMesh.indices);

		geometry.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
		if (normals.length > 0) {
			geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
		} else {
			geometry.computeVertexNormals();
		}
		geometry.setIndex(new THREE.BufferAttribute(indices, 1));

		const material = new THREE.MeshStandardMaterial({
			color: 0x6c9bd4,
			metalness: 0.1,
			roughness: 0.6
		});
		const mesh = new THREE.Mesh(geometry, material);
		scene.add(mesh);

		// Compute bounding sphere for camera positioning
		geometry.computeBoundingSphere();
		const center = geometry.boundingSphere.center;
		const radius = geometry.boundingSphere.radius || 1;

		// Camera along [1,1,1] direction, distance = 2x bounding sphere
		const dir = new THREE.Vector3(1, 1, 1).normalize();
		const camera = new THREE.PerspectiveCamera(45, width / height, 0.01, radius * 20);
		camera.position.copy(center).addScaledVector(dir, radius * 2.5);
		camera.lookAt(center);

		// Lighting
		const ambient = new THREE.AmbientLight(0xffffff, 0.5);
		scene.add(ambient);
		const directional = new THREE.DirectionalLight(0xffffff, 0.8);
		directional.position.copy(camera.position);
		scene.add(directional);

		renderer.render(scene, camera);

		return () => {
			geometry.dispose();
			material.dispose();
			renderer.dispose();
		};
	});
</script>

<div class="thumbnail-viewport" style="width: {width}px; height: {height}px;">
	{#if previewMesh}
		<canvas bind:this={canvasEl} {width} {height}></canvas>
	{:else}
		<div class="thumb-placeholder">
			<svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
				<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
				<polyline points="3.27 6.96 12 12.01 20.73 6.96"/>
				<line x1="12" y1="22.08" x2="12" y2="12"/>
			</svg>
		</div>
	{/if}
</div>

<style>
	.thumbnail-viewport {
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		background: var(--bg-primary, #1e1e2e);
	}

	.thumbnail-viewport canvas {
		display: block;
	}

	.thumb-placeholder {
		opacity: 0.4;
		color: var(--text-muted, #6c7086);
	}
</style>
