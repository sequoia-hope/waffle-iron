<script>
	import { Canvas } from '@threlte/core';
	import Scene from './Scene.svelte';
	import ViewCubeGizmo from './ViewCubeGizmo.svelte';
	import ViewCubeButtons from './ViewCubeButtons.svelte';
	import ConstraintMenu from '$lib/sketch/ConstraintMenu.svelte';

	let constraintMenuPos = $state({ x: 0, y: 0 });
	let constraintMenuVisible = $state(false);
</script>

<div class="viewport">
	<Canvas>
		<Scene />
		<ViewCubeGizmo />
	</Canvas>
	<ViewCubeButtons />
	<ConstraintMenu bind:menuPos={constraintMenuPos} bind:visible={constraintMenuVisible} />
</div>

<style>
	.viewport {
		width: 100%;
		height: 100%;
		background: #1a1a2e;
		position: relative;
	}
</style>
