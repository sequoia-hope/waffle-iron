<script>
	import { Canvas } from '@threlte/core';
	import CadModel from './CadModel.svelte';
	import CameraControls from './CameraControls.svelte';
	import Lighting from './Lighting.svelte';
</script>

<div class="viewport">
	<Canvas>
		<Lighting />
		<CameraControls />
		<CadModel />
	</Canvas>
</div>

<style>
	.viewport {
		width: 100%;
		height: 100%;
		background: #1a1a2e;
	}
</style>
