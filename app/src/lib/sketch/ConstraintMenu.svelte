<script>
	import {
		getSketchMode,
		getSketchSelection,
		getSketchEntities,
		getSketchPositions,
		addLocalConstraint,
		setSketchSelection
	} from '$lib/engine/store.svelte.js';
	import { getApplicableConstraints } from './constraintLogic.js';

	let { menuPos = $bindable({ x: 0, y: 0 }), visible = $bindable(false) } = $props();

	let sm = $derived(getSketchMode());
	let selection = $derived(getSketchSelection());
	let entities = $derived(getSketchEntities());
	let positions = $derived(getSketchPositions());

	/**
	 * Determine applicable constraints based on selection composition.
	 */
	let applicableConstraints = $derived.by(() => {
		const applicable = getApplicableConstraints(selection, entities, positions);
		const result = [];

		// Map from constraint logic to menu items
		if (applicable.coincident) result.push({ label: 'Coincident', build: applicable.coincident });
		if (applicable.horizontal) result.push({ label: 'Horizontal', build: applicable.horizontal });
		if (applicable.vertical) result.push({ label: 'Vertical', build: applicable.vertical });
		if (applicable.parallel) result.push({ label: 'Parallel', build: applicable.parallel });
		if (applicable.perpendicular) result.push({ label: 'Perpendicular', build: applicable.perpendicular });
		if (applicable.equal) result.push({ label: 'Equal', build: applicable.equal });
		if (applicable.tangent) result.push({ label: 'Tangent', build: applicable.tangent });
		if (applicable.midpoint) result.push({ label: 'Midpoint', build: applicable.midpoint });
		if (applicable.fix) result.push({ label: 'Fix', build: applicable.fix });
		if (applicable.distance) result.push({ label: 'Distance', build: applicable.distance });
		if (applicable.radius) result.push({ label: 'Radius', build: applicable.radius });

		return result;
	});

	function applyConstraint(item) {
		addLocalConstraint(item.build());
		visible = false;
	}

	function handleContextMenu(event) {
		if (!sm?.active) return;
		if (selection.size === 0) return;

		event.preventDefault();
		menuPos = { x: event.clientX, y: event.clientY };
		visible = true;
	}

	function closeMenu() {
		visible = false;
	}
</script>

<svelte:window oncontextmenu={handleContextMenu} onclick={closeMenu} />

{#if visible && applicableConstraints.length > 0}
	<div class="constraint-menu" style="left: {menuPos.x}px; top: {menuPos.y}px" role="menu">
		{#each applicableConstraints as item}
			<button class="constraint-item" onclick={() => applyConstraint(item)} role="menuitem">
				{item.label}
			</button>
		{/each}
	</div>
{/if}

<style>
	.constraint-menu {
		position: fixed;
		z-index: 1000;
		background: var(--bg-secondary, #252526);
		border: 1px solid var(--border-color, #3c3c3c);
		border-radius: 4px;
		padding: 4px 0;
		min-width: 140px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
	}

	.constraint-item {
		display: block;
		width: 100%;
		background: none;
		border: none;
		color: var(--text-primary, #cccccc);
		padding: 6px 12px;
		text-align: left;
		cursor: pointer;
		font-size: 12px;
	}

	.constraint-item:hover {
		background: var(--bg-hover, #2a2d2e);
	}
</style>
