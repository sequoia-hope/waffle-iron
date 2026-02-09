<script>
	import { Canvas } from '@threlte/core';
	import Scene from './Scene.svelte';
</script>

<div class="viewport">
	<Canvas>
		<Scene />
	</Canvas>
</div>

<style>
	.viewport {
		width: 100%;
		height: 100%;
		background: #1a1a2e;
	}
</style>
