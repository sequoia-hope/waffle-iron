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

	const getSafeInset = (prop) =>
		parseFloat(getComputedStyle(document.documentElement).getPropertyValue(prop)) || 0;

	let clampedPos = $derived.by(() => {
		const menuWidth = 160;
		const menuHeight = 200;
		const saiRight = getSafeInset('--sai-right');
		const saiLeft = getSafeInset('--sai-left');
		const maxX = window.innerWidth - menuWidth - 8 - saiRight;
		const maxY = window.innerHeight - menuHeight - 8;
		return {
			x: Math.min(pos.x, Math.max(saiLeft, maxX)),
			y: Math.min(pos.y, Math.max(0, maxY))
		};
	});
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

	async function handleNewSketch() {
		const refs = getSelectedRefs();
		if (refs.length > 0) {
			const plane = computeFacePlane(refs[0]);
			if (plane) {
				await enterSketchMode(plane.origin, plane.normal, refs[0]);
				visible = false;
				return;
			}
		}
		await enterSketchMode([0, 0, 0], [0, 0, 1]);
		visible = false;
	}

	function close() {
		visible = false;
	}

	function handleDismissPointerDown(e) {
		if (!visible) return;
		const menuEl = document.querySelector('[data-testid="ctx-menu"]');
		if (menuEl && menuEl.contains(e.target)) return;
		visible = false;
	}
</script>

<svelte:window onclick={close} onpointerdown={handleDismissPointerDown} />

{#if visible && !inSketch}
	<div
		class="ctx-menu"
		data-testid="ctx-menu"
		style="left: {clampedPos.x}px; top: {clampedPos.y}px"
		onclick={(e) => e.stopPropagation()}
	>
		{#if hasSelection}
			<button class="ctx-item" data-testid="ctx-sketch-on-face" onclick={handleNewSketch}>Sketch on Face</button>
			<div class="ctx-separator"></div>
		{/if}
		<button class="ctx-item" data-testid="ctx-new-sketch" onclick={handleNewSketch} disabled={!ready}>New Sketch (XY)</button>
		<div class="ctx-separator"></div>
		<button class="ctx-item" data-testid="ctx-fit-all" onclick={handleFitAll}>Fit All (F)</button>
		<button class="ctx-item" data-testid="ctx-view-front" onclick={() => handleSnapView('front')}>Front View</button>
		<button class="ctx-item" data-testid="ctx-view-top" onclick={() => handleSnapView('top')}>Top View</button>
		<button class="ctx-item" data-testid="ctx-view-right" onclick={() => handleSnapView('right')}>Right View</button>
		<button class="ctx-item" data-testid="ctx-view-iso" onclick={() => handleSnapView('iso')}>Isometric</button>
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
