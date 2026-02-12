<script>
	import { onMount } from 'svelte';
	import Toolbar from '$lib/ui/Toolbar.svelte';
	import FeatureTree from '$lib/ui/FeatureTree.svelte';
	import PropertyEditor from '$lib/ui/PropertyEditor.svelte';
	import StatusBar from '$lib/ui/StatusBar.svelte';
	import Viewport from '$lib/viewport/Viewport.svelte';
	import ExtrudeDialog from '$lib/ui/ExtrudeDialog.svelte';
	import RevolveDialog from '$lib/ui/RevolveDialog.svelte';
	import SketchPlaneDialog from '$lib/ui/SketchPlaneDialog.svelte';
	import { initEngine } from '$lib/engine/store.svelte.js';

	let leftWidth = $state(200);
	let rightWidth = $state(250);

	/** @type {'left' | 'right' | null} */
	let resizing = $state(null);

	onMount(() => {
		initEngine();

		function onMouseMove(e) {
			if (!resizing) return;
			if (resizing === 'left') {
				leftWidth = Math.max(120, Math.min(400, e.clientX));
			} else if (resizing === 'right') {
				rightWidth = Math.max(150, Math.min(450, window.innerWidth - e.clientX));
			}
		}

		function onMouseUp() {
			resizing = null;
			document.body.style.cursor = '';
			document.body.style.userSelect = '';
		}

		window.addEventListener('mousemove', onMouseMove);
		window.addEventListener('mouseup', onMouseUp);
		return () => {
			window.removeEventListener('mousemove', onMouseMove);
			window.removeEventListener('mouseup', onMouseUp);
		};
	});

	function startResize(side) {
		resizing = side;
		document.body.style.cursor = 'col-resize';
		document.body.style.userSelect = 'none';
	}
</script>

<div
	class="app-shell"
	style="grid-template-columns: {leftWidth}px auto 1fr auto {rightWidth}px"
>
	<div class="toolbar-area">
		<Toolbar />
	</div>
	<div class="left-panel">
		<FeatureTree />
	</div>
	<div
		class="divider"
		class:active={resizing === 'left'}
		onmousedown={() => startResize('left')}
		role="separator"
		aria-orientation="vertical"
		tabindex="-1"
	></div>
	<div class="viewport-area">
		<Viewport />
	</div>
	<div
		class="divider"
		class:active={resizing === 'right'}
		onmousedown={() => startResize('right')}
		role="separator"
		aria-orientation="vertical"
		tabindex="-1"
	></div>
	<div class="right-panel">
		<PropertyEditor />
	</div>
	<div class="statusbar-area">
		<StatusBar />
	</div>
</div>

<ExtrudeDialog />
<RevolveDialog />
<SketchPlaneDialog />

<style>
	.app-shell {
		display: grid;
		grid-template-rows: var(--toolbar-height) 1fr var(--statusbar-height);
		/* columns set inline via style binding */
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
	}

	.divider {
		grid-row: 2;
		width: 4px;
		cursor: col-resize;
		background: var(--border-color);
		transition: background 0.15s;
	}

	.divider:hover,
	.divider.active {
		background: var(--accent);
	}

	.viewport-area {
		grid-row: 2;
		overflow: hidden;
		position: relative;
	}

	.right-panel {
		grid-column: 5;
		grid-row: 2;
		overflow-y: auto;
	}

	.statusbar-area {
		grid-column: 1 / -1;
		grid-row: 3;
	}
</style>
