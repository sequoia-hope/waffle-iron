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
	import Icon from '$lib/ui/icons/Icon.svelte';

	let { menuPos = $bindable({ x: 0, y: 0 }), visible = $bindable(false) } = $props();

	let sm = $derived(getSketchMode());

	const getSafeInset = (prop) =>
		parseFloat(getComputedStyle(document.documentElement).getPropertyValue(prop)) || 0;

	let clampedPos = $derived.by(() => {
		const menuWidth = 150;
		const menuHeight = 300;
		const saiRight = getSafeInset('--sai-right');
		const saiLeft = getSafeInset('--sai-left');
		const maxX = window.innerWidth - menuWidth - 8 - saiRight;
		const maxY = window.innerHeight - menuHeight - 8;
		return {
			x: Math.min(menuPos.x, Math.max(saiLeft, maxX)),
			y: Math.min(menuPos.y, Math.max(0, maxY))
		};
	});
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
		if (applicable.coincident) result.push({ label: 'Coincident', icon: 'constraint-coincident', build: applicable.coincident });
		if (applicable.horizontal) result.push({ label: 'Horizontal', icon: 'constraint-horizontal', build: applicable.horizontal });
		if (applicable.vertical) result.push({ label: 'Vertical', icon: 'constraint-vertical', build: applicable.vertical });
		if (applicable.parallel) result.push({ label: 'Parallel', icon: 'constraint-parallel', build: applicable.parallel });
		if (applicable.perpendicular) result.push({ label: 'Perpendicular', icon: 'constraint-perpendicular', build: applicable.perpendicular });
		if (applicable.equal) result.push({ label: 'Equal', icon: 'constraint-equal', build: applicable.equal });
		if (applicable.tangent) result.push({ label: 'Tangent', icon: 'constraint-tangent', build: applicable.tangent });
		if (applicable.midpoint) result.push({ label: 'Midpoint', icon: 'constraint-midpoint', build: applicable.midpoint });
		if (applicable.fix) result.push({ label: 'Fix', icon: 'constraint-fix', build: applicable.fix });
		if (applicable.distance) result.push({ label: 'Distance', icon: 'dim-distance', build: applicable.distance });
		if (applicable.radius) result.push({ label: 'Radius', icon: 'dim-radius', build: applicable.radius });
		if (applicable.angle) result.push({ label: 'Angle', icon: 'constraint-angle', build: applicable.angle });
		if (applicable.symmetric) result.push({ label: 'Symmetric', icon: 'constraint-symmetric', build: applicable.symmetric });
		if (applicable.symmetricH) result.push({ label: 'Symmetric H', icon: 'constraint-symmetric-h', build: applicable.symmetricH });
		if (applicable.symmetricV) result.push({ label: 'Symmetric V', icon: 'constraint-symmetric-v', build: applicable.symmetricV });
		if (applicable.pointOnLine) result.push({ label: 'Point on Line', icon: 'constraint-point-on-line', build: applicable.pointOnLine });
		if (applicable.pointOnCircle) result.push({ label: 'Point on Circle', icon: 'constraint-point-on-circle', build: applicable.pointOnCircle });
		if (applicable.equalRadius) result.push({ label: 'Equal Radius', icon: 'constraint-equal-radius', build: applicable.equalRadius });
		if (applicable.lengthRatio) result.push({ label: 'Length Ratio', icon: 'constraint-length-ratio', build: applicable.lengthRatio });
		if (applicable.pointLineDistance) result.push({ label: 'Point-Line Distance', icon: 'dim-pointline', build: applicable.pointLineDistance });
		if (applicable.diameter) result.push({ label: 'Diameter', icon: 'dim-diameter', build: applicable.diameter });

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
	<div class="constraint-menu" style="left: {clampedPos.x}px; top: {clampedPos.y}px" role="menu">
		{#each applicableConstraints as item}
			<button class="constraint-item" onclick={() => applyConstraint(item)} role="menuitem">
				<Icon name={item.icon} />{item.label}
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
		display: flex;
		align-items: center;
		gap: 8px;
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
