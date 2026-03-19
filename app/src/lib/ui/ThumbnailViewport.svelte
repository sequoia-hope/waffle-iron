<script>
	import { onMount } from 'svelte';
	import * as THREE from 'three';

	let { previewMesh = null, width = 240, height = 180 } = $props();

	let canvasEl;
	let hovered = $state(false);
	let renderer, scene, camera, meshObj, geometry, material, frameId;
	let angle = 0;
	let center = new THREE.Vector3();
	let radius = 1;

	onMount(() => {
		if (!previewMesh || !canvasEl) return;

		renderer = new THREE.WebGLRenderer({
			canvas: canvasEl,
			antialias: true,
			alpha: true,
			powerPreference: 'low-power'
		});
		renderer.setSize(width, height);
		renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

		scene = new THREE.Scene();
		scene.background = new THREE.Color(0x1e1e2e);

		// Build mesh from preview data
		geometry = new THREE.BufferGeometry();
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
		geometry.computeBoundingSphere();

		center = geometry.boundingSphere.center.clone();
		radius = geometry.boundingSphere.radius || 1;

		material = new THREE.MeshStandardMaterial({
			color: 0x6c9bd4,
			metalness: 0.15,
			roughness: 0.5
		});
		meshObj = new THREE.Mesh(geometry, material);
		scene.add(meshObj);

		// Wireframe edges for CAD look
		const edges = new THREE.EdgesGeometry(geometry, 30);
		const edgeMat = new THREE.LineBasicMaterial({ color: 0x3a4a6b, opacity: 0.5, transparent: true });
		const edgeMesh = new THREE.LineSegments(edges, edgeMat);
		scene.add(edgeMesh);

		// Camera
		camera = new THREE.PerspectiveCamera(40, width / height, 0.01, radius * 40);

		// Lighting
		const ambient = new THREE.AmbientLight(0xffffff, 0.6);
		scene.add(ambient);
		const key = new THREE.DirectionalLight(0xffffff, 0.8);
		key.position.set(1, 2, 1.5).normalize().multiplyScalar(radius * 5);
		scene.add(key);
		const fill = new THREE.DirectionalLight(0x8899cc, 0.3);
		fill.position.set(-1, 0.5, -1).normalize().multiplyScalar(radius * 5);
		scene.add(fill);

		// Initial render at angle 0
		updateCamera();
		renderer.render(scene, camera);

		return () => {
			if (frameId) cancelAnimationFrame(frameId);
			edges.dispose();
			edgeMat.dispose();
			geometry.dispose();
			material.dispose();
			renderer.dispose();
		};
	});

	function updateCamera() {
		if (!camera) return;
		const dist = radius * 2.8;
		camera.position.set(
			center.x + dist * Math.cos(angle) * Math.cos(0.5),
			center.y + dist * Math.sin(0.5),
			center.z + dist * Math.sin(angle) * Math.cos(0.5)
		);
		camera.lookAt(center);
	}

	function animate() {
		if (!hovered) {
			frameId = null;
			return;
		}
		angle += 0.012;
		updateCamera();
		renderer.render(scene, camera);
		frameId = requestAnimationFrame(animate);
	}

	function handleEnter() {
		hovered = true;
		if (!frameId && renderer) {
			frameId = requestAnimationFrame(animate);
		}
	}

	function handleLeave() {
		hovered = false;
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="thumbnail-viewport"
	style="width: {width}px; height: {height}px;"
	onmouseenter={handleEnter}
	onmouseleave={handleLeave}
>
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
