<script>
	import { Canvas } from '@threlte/core';
	import Scene from './Scene.svelte';
	import ViewCubeGizmo from './ViewCubeGizmo.svelte';
	import ViewCubeButtons from './ViewCubeButtons.svelte';
</script>

<div class="viewport">
	<Canvas>
		<Scene />
		<ViewCubeGizmo />
	</Canvas>
	<ViewCubeButtons />
</div>

<style>
	.viewport {
		width: 100%;
		height: 100%;
		background: #1a1a2e;
		position: relative;
	}
</style>
