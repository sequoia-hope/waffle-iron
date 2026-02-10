<script>
	import {
		getSelectedRefs,
		getSketchMode,
		enterSketchMode,
		isEngineReady,
		computeFacePlane
	} from '$lib/engine/store.svelte.js';

	let { visible = $bindable(false), pos = $bindable({ x: 0, y: 0 }) } = $props();

	let ready = $derived(isEngineReady());
	let inSketch = $derived(getSketchMode()?.active ?? false);
	let hasSelection = $derived(getSelectedRefs().length > 0);

	function handleFitAll() {
		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f' }));
		visible = false;
	}

	function handleSnapView(name) {
		window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: name } }));
		visible = false;
	}

	function handleNewSketch() {
		const refs = getSelectedRefs();
		if (refs.length > 0) {
			const plane = computeFacePlane(refs[0]);
			if (plane) {
				enterSketchMode(plane.origin, plane.normal);
				visible = false;
				return;
			}
		}
		enterSketchMode([0, 0, 0], [0, 0, 1]);
		visible = false;
	}

	function close() {
		visible = false;
	}
</script>

<svelte:window onclick={close} />

{#if visible && !inSketch}
	<div
		class="ctx-menu"
		style="left: {pos.x}px; top: {pos.y}px"
		onclick={(e) => e.stopPropagation()}
	>
		{#if hasSelection}
			<button class="ctx-item" onclick={handleNewSketch}>Sketch on Face</button>
			<div class="ctx-separator"></div>
		{/if}
		<button class="ctx-item" onclick={handleNewSketch} disabled={!ready}>New Sketch (XY)</button>
		<div class="ctx-separator"></div>
		<button class="ctx-item" onclick={handleFitAll}>Fit All (F)</button>
		<button class="ctx-item" onclick={() => handleSnapView('front')}>Front View</button>
		<button class="ctx-item" onclick={() => handleSnapView('top')}>Top View</button>
		<button class="ctx-item" onclick={() => handleSnapView('right')}>Right View</button>
		<button class="ctx-item" onclick={() => handleSnapView('iso')}>Isometric</button>
	</div>
{/if}

<style>
	.ctx-menu {
		position: fixed;
		background: var(--bg-tertiary, #2a2a3e);
		border: 1px solid var(--border-color, #444);
		border-radius: 4px;
		padding: 4px 0;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
		min-width: 150px;
	}

	.ctx-item {
		display: block;
		width: 100%;
		background: transparent;
		border: none;
		color: var(--text-primary, #eee);
		font-size: 12px;
		padding: 5px 16px;
		cursor: pointer;
		text-align: left;
	}

	.ctx-item:hover:not(:disabled) {
		background: var(--accent, #0078d4);
		color: white;
	}

	.ctx-item:disabled {
		color: var(--text-muted, #666);
		cursor: default;
	}

	.ctx-separator {
		height: 1px;
		background: var(--border-color, #444);
		margin: 3px 0;
	}
</style>
