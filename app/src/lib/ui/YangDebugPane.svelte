<script>
	import { onMount, onDestroy } from 'svelte';
	import * as THREE from 'three';
	import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
	import {
		getYangDebugState,
		hideYangDebugPane,
		selectYangDebugFeature,
		setYangDebugStageIndex,
	} from '$lib/engine/store.svelte.js';
	import { bottomSheetResize } from './bottomSheetResize.js';

	let state = $derived(getYangDebugState());
	let featureTree = $derived(getFeatureTree());

	function getFeatureTree() {
		if (typeof window !== 'undefined' && window.__waffle?.getFeatureTree) {
			return window.__waffle.getFeatureTree();
		}
		return { features: [] };
	}

	let canvasEl;
	let renderer = null;
	let scene = null;
	let camera = null;
	let controls = null;
	let geometry = null;
	let material = null;
	let mesh = null;
	let edges = null;
	let edgeMat = null;
	let edgeMesh = null;
	let frameId = null;
	let canvasWidth = 360;
	let canvasHeight = 280;

	function startRenderLoop() {
		if (frameId !== null) return;
		const loop = () => {
			frameId = requestAnimationFrame(loop);
			if (controls) controls.update();
			if (renderer && scene && camera) renderer.render(scene, camera);
		};
		frameId = requestAnimationFrame(loop);
	}

	function stopRenderLoop() {
		if (frameId !== null) {
			cancelAnimationFrame(frameId);
			frameId = null;
		}
	}

	function disposeMesh() {
		if (mesh && scene) scene.remove(mesh);
		if (edgeMesh && scene) scene.remove(edgeMesh);
		if (geometry) geometry.dispose();
		if (material) material.dispose();
		if (edges) edges.dispose();
		if (edgeMat) edgeMat.dispose();
		geometry = null;
		material = null;
		mesh = null;
		edges = null;
		edgeMat = null;
		edgeMesh = null;
	}

	onMount(() => {
		if (!canvasEl) return;
		renderer = new THREE.WebGLRenderer({
			canvas: canvasEl,
			antialias: true,
			alpha: true,
			powerPreference: 'low-power',
		});
		renderer.setSize(canvasWidth, canvasHeight);
		renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));

		scene = new THREE.Scene();
		scene.background = new THREE.Color(0x1e1e2e);

		camera = new THREE.PerspectiveCamera(40, canvasWidth / canvasHeight, 0.001, 10000);
		camera.position.set(2, 2, 2);
		camera.lookAt(0, 0, 0);

		const ambient = new THREE.AmbientLight(0xffffff, 0.6);
		scene.add(ambient);
		const key = new THREE.DirectionalLight(0xffffff, 0.8);
		key.position.set(1, 2, 1.5);
		scene.add(key);

		controls = new OrbitControls(camera, canvasEl);
		controls.enableDamping = true;

		startRenderLoop();
	});

	onDestroy(() => {
		stopRenderLoop();
		disposeMesh();
		if (controls) controls.dispose();
		if (renderer) renderer.dispose();
		controls = null;
		renderer = null;
		scene = null;
		camera = null;
	});

	$effect(() => {
		// Recompute mesh when (featureId, stageIndex) changes.
		const fid = state.featureId;
		const si = state.stageIndex;
		const cap = fid ? state.captures.get(fid) : null;
		if (!scene) return;
		disposeMesh();
		if (!cap || !Array.isArray(cap.stages) || cap.stages.length === 0) return;
		const stage = cap.stages[Math.max(0, Math.min(si, cap.stages.length - 1))];
		if (!stage) return;

		const verts = new Float32Array(stage.vertices || []);
		const indices = new Uint32Array(stage.indices || []);
		if (verts.length === 0 || indices.length === 0) return;

		geometry = new THREE.BufferGeometry();
		geometry.setAttribute('position', new THREE.BufferAttribute(verts, 3));
		geometry.setIndex(new THREE.BufferAttribute(indices, 1));
		geometry.computeVertexNormals();
		geometry.computeBoundingSphere();

		material = new THREE.MeshStandardMaterial({
			color: 0x6c9bd4,
			metalness: 0.1,
			roughness: 0.6,
			side: THREE.DoubleSide,
		});
		mesh = new THREE.Mesh(geometry, material);
		scene.add(mesh);

		edges = new THREE.EdgesGeometry(geometry, 30);
		edgeMat = new THREE.LineBasicMaterial({ color: 0x3a4a6b, opacity: 0.6, transparent: true });
		edgeMesh = new THREE.LineSegments(edges, edgeMat);
		scene.add(edgeMesh);

		// Frame the camera to the mesh
		if (geometry.boundingSphere) {
			const c = geometry.boundingSphere.center;
			const r = geometry.boundingSphere.radius || 1;
			camera.position.set(c.x + r * 2.5, c.y + r * 2.5, c.z + r * 2.5);
			camera.near = Math.max(r * 0.001, 0.001);
			camera.far = Math.max(r * 100, 1000);
			camera.updateProjectionMatrix();
			if (controls) {
				controls.target.copy(c);
				controls.update();
			}
		}
	});

	let currentCapture = $derived(state.featureId ? state.captures.get(state.featureId) : null);
	let currentStage = $derived.by(() => {
		if (!currentCapture || !Array.isArray(currentCapture.stages)) return null;
		const i = Math.max(0, Math.min(state.stageIndex, currentCapture.stages.length - 1));
		return currentCapture.stages[i] ?? null;
	});
	let isFailedHere = $derived(
		currentCapture &&
			typeof currentCapture.failed_at_stage === 'number' &&
			currentCapture.failed_at_stage === state.stageIndex
	);

	function handleFeatureChange(e) {
		const id = e.currentTarget.value;
		if (id) selectYangDebugFeature(id);
	}

	function handleStageChange(e) {
		const i = parseInt(e.currentTarget.value, 10);
		if (!Number.isNaN(i)) setYangDebugStageIndex(i);
	}

	let features = $derived(featureTree?.features ?? []);
</script>

{#if state.visible}
<div class="yang-debug-pane" data-testid="yang-debug-pane">
	<div class="ydp-header" use:bottomSheetResize>
		<span class="ydp-title">Yang Debug</span>
		<span class="ydp-armed-indicator" title="Capture is armed while pane is open">capture armed</span>
		<div class="ydp-header-actions">
			<button class="ydp-icon-btn" title="Close" data-testid="yang-debug-close"
				onclick={() => hideYangDebugPane()}>&#x2715;</button>
		</div>
	</div>

	<div class="ydp-body">
		<div class="ydp-row">
			<label class="ydp-label" for="yang-debug-feature-select">Feature</label>
			<select
				id="yang-debug-feature-select"
				class="ydp-select"
				data-testid="yang-debug-feature-select"
				value={state.featureId ?? ''}
				onchange={handleFeatureChange}
			>
				<option value="" disabled>Select a feature…</option>
				{#each features as feature (feature.id)}
					<option value={feature.id}>{feature.name}</option>
				{/each}
			</select>
		</div>

		<div class="ydp-row">
			<label class="ydp-label" for="yang-debug-stage-select">Stage</label>
			<select
				id="yang-debug-stage-select"
				class="ydp-select"
				data-testid="yang-debug-stage-select"
				value={String(state.stageIndex)}
				disabled={!currentCapture || !currentCapture.stages?.length}
				onchange={handleStageChange}
			>
				{#if currentCapture && currentCapture.stages?.length}
					{#each currentCapture.stages as stage, i (i)}
						<option value={String(i)}>
							{i}: {stage.stage_tag ?? '?'}
						</option>
					{/each}
				{:else}
					<option value="0">No stages captured</option>
				{/if}
			</select>
		</div>

		<div class="ydp-canvas-wrap" class:failed={isFailedHere}>
			<canvas bind:this={canvasEl} width={canvasWidth} height={canvasHeight}></canvas>
			{#if isFailedHere}
				<div class="ydp-failed-marker" data-testid="yang-debug-failed-marker">FAILED HERE</div>
			{/if}
		</div>

		{#if currentStage}
			<div class="ydp-stage-info">
				<div><strong>Tag:</strong> {currentStage.stage_tag ?? '?'}</div>
				<div><strong>Verts:</strong> {(currentStage.vertices?.length ?? 0) / 3}</div>
				<div><strong>Tris:</strong> {(currentStage.indices?.length ?? 0) / 3}</div>
				{#if currentCapture && typeof currentCapture.failed_at_stage === 'number'}
					<div><strong>Failed at stage:</strong> {currentCapture.failed_at_stage}</div>
				{/if}
			</div>
		{:else if state.featureId}
			<div class="ydp-empty">No capture data for this feature yet. Edit it to re-trigger Yang.</div>
		{:else}
			<div class="ydp-empty">Select a feature to inspect Yang stages.</div>
		{/if}
	</div>
</div>
{/if}

<style>
	.yang-debug-pane {
		position: absolute;
		top: 0;
		right: 0;
		width: 380px;
		height: 100%;
		background: var(--bg-secondary);
		border-left: 1px solid var(--border-color);
		/* Below ExtrudeDialog (z-index 50) so feature-create dialogs remain
		   clickable when the pane is open; still above the bare viewport. */
		z-index: 40;
		display: flex;
		flex-direction: column;
		font-size: 12px;
		color: var(--text-primary);
	}

	.ydp-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border-bottom: 1px solid var(--border-color);
	}

	.ydp-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary);
	}

	.ydp-armed-indicator {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: #4caf50;
		background: rgba(76, 175, 80, 0.12);
		padding: 1px 6px;
		border-radius: 8px;
		font-family: monospace;
	}

	.ydp-header-actions {
		margin-left: auto;
		display: flex;
		gap: 4px;
	}

	.ydp-icon-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 14px;
		padding: 2px 6px;
		border-radius: 3px;
	}

	.ydp-icon-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.ydp-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px;
		overflow-y: auto;
	}

	.ydp-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.ydp-label {
		min-width: 60px;
		font-size: 11px;
		color: var(--text-secondary);
	}

	.ydp-select {
		flex: 1;
		padding: 4px 6px;
		font-size: 11px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		outline: none;
	}

	.ydp-select:focus {
		border-color: var(--accent);
	}

	.ydp-canvas-wrap {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		background: #1e1e2e;
		border: 1px solid var(--border-color);
		border-radius: 4px;
		padding: 4px;
	}

	.ydp-canvas-wrap.failed {
		border-color: #f44336;
		box-shadow: 0 0 0 1px #f44336;
	}

	.ydp-canvas-wrap canvas {
		display: block;
		max-width: 100%;
	}

	.ydp-failed-marker {
		position: absolute;
		top: 6px;
		left: 6px;
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.5px;
		padding: 2px 6px;
		background: rgba(244, 67, 54, 0.85);
		color: white;
		border-radius: 3px;
		font-family: monospace;
	}

	.ydp-stage-info {
		font-size: 11px;
		color: var(--text-secondary);
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 6px 8px;
		background: var(--bg-tertiary, var(--bg-primary));
		border-radius: 3px;
	}

	.ydp-empty {
		font-size: 11px;
		color: var(--text-muted, var(--text-secondary));
		font-style: italic;
		padding: 8px;
		text-align: center;
	}

	@media (max-width: 768px) {
		.yang-debug-pane {
			width: 100%;
			height: 60vh;
			top: auto;
			bottom: 0;
			left: 0;
			right: 0;
			position: fixed;
			border-radius: 12px 12px 0 0;
			border-left: none;
			border-top: 1px solid var(--border-color);
			z-index: 150;
		}

		.ydp-header {
			cursor: grab;
		}

		.ydp-header::before {
			content: '';
			display: block;
			width: 32px;
			height: 4px;
			background: var(--text-muted);
			opacity: 0.4;
			border-radius: 2px;
			flex-basis: 100%;
			margin: 2px auto 4px;
		}
	}
</style>
