<script>
	import { Canvas } from '@threlte/core';
	import Scene from './Scene.svelte';
	import ViewCubeGizmo from './ViewCubeGizmo.svelte';
	import ViewCubeButtons from './ViewCubeButtons.svelte';
	import ConstraintMenu from '$lib/sketch/ConstraintMenu.svelte';
	import ViewportContextMenu from './ViewportContextMenu.svelte';

	let constraintMenuPos = $state({ x: 0, y: 0 });
	let constraintMenuVisible = $state(false);
	let ctxMenuPos = $state({ x: 0, y: 0 });
	let ctxMenuVisible = $state(false);

	function handleContextMenu(e) {
		e.preventDefault();
		ctxMenuPos = { x: e.clientX, y: e.clientY };
		ctxMenuVisible = true;
	}
</script>

<div class="viewport" oncontextmenu={handleContextMenu}>
	<Canvas>
		<Scene />
		<ViewCubeGizmo />
	</Canvas>
	<ViewCubeButtons />
	<ConstraintMenu bind:menuPos={constraintMenuPos} bind:visible={constraintMenuVisible} />
	<ViewportContextMenu bind:pos={ctxMenuPos} bind:visible={ctxMenuVisible} />
</div>

<style>
	.viewport {
		width: 100%;
		height: 100%;
		background: #1a1a2e;
		position: relative;
	}
</style>
