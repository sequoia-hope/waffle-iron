<script>
	import { onMount } from 'svelte';
	import Toolbar from '$lib/ui/Toolbar.svelte';
	import FeatureTree from '$lib/ui/FeatureTree.svelte';
	import PropertyEditor from '$lib/ui/PropertyEditor.svelte';
	import StatusBar from '$lib/ui/StatusBar.svelte';
	import Viewport from '$lib/viewport/Viewport.svelte';
	import { initEngine } from '$lib/engine/store.svelte.js';

	onMount(() => {
		initEngine();
	});
</script>

<div class="app-shell">
	<div class="toolbar-area">
		<Toolbar />
	</div>
	<div class="left-panel">
		<FeatureTree />
	</div>
	<div class="viewport-area">
		<Viewport />
	</div>
	<div class="right-panel">
		<PropertyEditor />
	</div>
	<div class="statusbar-area">
		<StatusBar />
	</div>
</div>

<style>
	.app-shell {
		display: grid;
		grid-template-rows: var(--toolbar-height) 1fr var(--statusbar-height);
		grid-template-columns: var(--panel-width-left) 1fr var(--panel-width-right);
		height: 100vh;
		width: 100vw;
	}

	.toolbar-area {
		grid-column: 1 / -1;
		grid-row: 1;
	}

	.left-panel {
		grid-column: 1;
		grid-row: 2;
		overflow-y: auto;
		border-right: 1px solid var(--border-color);
	}

	.viewport-area {
		grid-column: 2;
		grid-row: 2;
		overflow: hidden;
		position: relative;
	}

	.right-panel {
		grid-column: 3;
		grid-row: 2;
		overflow-y: auto;
		border-left: 1px solid var(--border-color);
	}

	.statusbar-area {
		grid-column: 1 / -1;
		grid-row: 3;
	}
</style>
