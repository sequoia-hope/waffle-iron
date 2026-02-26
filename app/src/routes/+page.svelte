<script>
	import { onMount } from 'svelte';
	import Toolbar from '$lib/ui/Toolbar.svelte';
	import FeatureTree from '$lib/ui/FeatureTree.svelte';
	import PropertyEditor from '$lib/ui/PropertyEditor.svelte';
	import StatusBar from '$lib/ui/StatusBar.svelte';
	import Viewport from '$lib/viewport/Viewport.svelte';
	import RevolveDialog from '$lib/ui/RevolveDialog.svelte';
	import SketchPlaneDialog from '$lib/ui/SketchPlaneDialog.svelte';
	import ToastContainer from '$lib/ui/ToastContainer.svelte';
	import { initEngine, getMobileLayout, setMobileLayout, getMobileActivePanel, toggleMobilePanel } from '$lib/engine/store.svelte.js';
	import TestCaseBrowser from '$lib/ui/TestCaseBrowser.svelte';
	import SaveTestCaseDialog from '$lib/ui/SaveTestCaseDialog.svelte';

	let leftWidth = $state(200);
	let rightWidth = $state(250);
	let isMobile = $derived(getMobileLayout());
	let activePanel = $derived(getMobileActivePanel());

	/** @type {'left' | 'right' | null} */
	let resizing = $state(null);

	onMount(() => {
		initEngine();

		// Responsive layout listener
		const mql = window.matchMedia('(max-width: 768px)');
		setMobileLayout(mql.matches);
		function onMediaChange(e) { setMobileLayout(e.matches); }
		mql.addEventListener('change', onMediaChange);

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
			mql.removeEventListener('change', onMediaChange);
			window.removeEventListener('mousemove', onMouseMove);
			window.removeEventListener('mouseup', onMouseUp);
		};
	});

	function startResize(side) {
		resizing = side;
		document.body.style.cursor = 'col-resize';
		document.body.style.userSelect = 'none';
	}

	function closeMobilePanel() {
		// Close whichever panel is open
		if (activePanel) toggleMobilePanel(activePanel);
	}
</script>

{#if isMobile}
<div class="app-shell mobile">
	<div class="toolbar-area">
		<Toolbar />
	</div>
	<div class="viewport-area">
		<Viewport />
		<button class="fab fab-left" data-testid="mobile-toggle-tree" onclick={() => toggleMobilePanel('left')} title="Feature Tree">&#x2630;</button>
		<button class="fab fab-right" data-testid="mobile-toggle-props" onclick={() => toggleMobilePanel('right')} title="Properties">&#x2699;</button>
	</div>
	{#if activePanel}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="mobile-backdrop" onclick={closeMobilePanel}></div>
	{/if}
	<div class="mobile-panel mobile-panel-left" class:open={activePanel === 'left'}>
		<FeatureTree />
	</div>
	<div class="mobile-panel mobile-panel-right" class:open={activePanel === 'right'}>
		<PropertyEditor />
	</div>
	<div class="statusbar-area">
		<StatusBar />
	</div>
</div>
{:else}
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
{/if}

<RevolveDialog />
<SketchPlaneDialog />
<ToastContainer />
<TestCaseBrowser />
<SaveTestCaseDialog />

<style>
	.app-shell {
		display: grid;
		grid-template-rows: var(--toolbar-height) 1fr var(--statusbar-height);
		/* columns set inline via style binding */
		height: 100vh;
		height: 100dvh;
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

	/* Mobile layout */
	.app-shell.mobile {
		grid-template-columns: 1fr;
		position: relative;
	}

	.app-shell.mobile .toolbar-area {
		grid-column: 1;
		overflow: hidden;
	}

	.app-shell.mobile .viewport-area {
		grid-column: 1;
		min-height: 0;
		overflow: hidden;
	}

	.app-shell.mobile .statusbar-area {
		grid-column: 1;
		overflow: hidden;
	}

	.mobile-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		z-index: 99;
	}

	.mobile-panel {
		position: fixed;
		top: var(--toolbar-height);
		bottom: var(--statusbar-height);
		width: min(260px, 75vw);
		background: var(--bg-secondary);
		z-index: 100;
		overflow-y: auto;
		transition: transform 0.2s ease;
		-webkit-overflow-scrolling: touch;
	}

	.mobile-panel-left {
		left: 0;
		transform: translateX(-100%);
		border-right: 1px solid var(--border-color);
	}

	.mobile-panel-left.open {
		transform: translateX(0);
	}

	.mobile-panel-right {
		right: 0;
		transform: translateX(100%);
		border-left: 1px solid var(--border-color);
	}

	.mobile-panel-right.open {
		transform: translateX(0);
	}

	@media (max-width: 960px) and (orientation: landscape) {
		.mobile-panel {
			width: min(220px, 50vw);
		}
	}

	.fab {
		position: absolute;
		top: 50%;
		transform: translateY(-50%);
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 1px solid var(--border-color);
		background: var(--bg-tertiary);
		color: var(--text-primary);
		font-size: 16px;
		line-height: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		z-index: 20;
		opacity: 0.6;
		transition: opacity 0.15s;
	}

	.fab:hover, .fab:active {
		opacity: 1;
	}

	.fab-left {
		left: 4px;
	}

	.fab-right {
		right: 4px;
	}
</style>
