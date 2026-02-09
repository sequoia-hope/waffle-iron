<script>
	import {
		getSketchMode,
		getSketchSelection,
		getSketchEntities,
		addLocalConstraint,
		setSketchSelection
	} from '$lib/engine/store.svelte.js';

	let { menuPos = $bindable({ x: 0, y: 0 }), visible = $bindable(false) } = $props();

	let sm = $derived(getSketchMode());
	let selection = $derived(getSketchSelection());
	let entities = $derived(getSketchEntities());

	/**
	 * Determine which entities are in the selection by type.
	 */
	let selectedEntities = $derived.by(() => {
		const sel = [...selection];
		return sel.map(id => entities.find(e => e.id === id)).filter(Boolean);
	});

	let points = $derived(selectedEntities.filter(e => e.type === 'Point'));
	let lines = $derived(selectedEntities.filter(e => e.type === 'Line'));
	let circles = $derived(selectedEntities.filter(e => e.type === 'Circle'));
	let arcs = $derived(selectedEntities.filter(e => e.type === 'Arc'));

	/**
	 * Determine applicable constraints based on selection composition.
	 */
	let applicableConstraints = $derived.by(() => {
		const result = [];

		// 2 points
		if (points.length === 2 && lines.length === 0 && circles.length === 0 && arcs.length === 0) {
			result.push({ label: 'Coincident', build: () => ({ type: 'Coincident', point_a: points[0].id, point_b: points[1].id }) });
			result.push({ label: 'Distance', build: () => ({ type: 'Distance', entity_a: points[0].id, entity_b: points[1].id, value: 1.0 }) });
		}

		// 2 lines
		if (lines.length === 2 && points.length === 0 && circles.length === 0 && arcs.length === 0) {
			result.push({ label: 'Parallel', build: () => ({ type: 'Parallel', line_a: lines[0].id, line_b: lines[1].id }) });
			result.push({ label: 'Perpendicular', build: () => ({ type: 'Perpendicular', line_a: lines[0].id, line_b: lines[1].id }) });
			result.push({ label: 'Equal', build: () => ({ type: 'Equal', entity_a: lines[0].id, entity_b: lines[1].id }) });
			result.push({ label: 'Angle', build: () => ({ type: 'Angle', line_a: lines[0].id, line_b: lines[1].id, value_degrees: 45 }) });
		}

		// 1 line
		if (lines.length === 1 && points.length === 0 && circles.length === 0 && arcs.length === 0) {
			result.push({ label: 'Horizontal', build: () => ({ type: 'Horizontal', entity: lines[0].id }) });
			result.push({ label: 'Vertical', build: () => ({ type: 'Vertical', entity: lines[0].id }) });
		}

		// 1 point + 1 line
		if (points.length === 1 && lines.length === 1 && circles.length === 0 && arcs.length === 0) {
			result.push({ label: 'On Entity', build: () => ({ type: 'OnEntity', point: points[0].id, entity: lines[0].id }) });
			result.push({ label: 'Midpoint', build: () => ({ type: 'Midpoint', point: points[0].id, line: lines[0].id }) });
			result.push({ label: 'Distance', build: () => ({ type: 'Distance', entity_a: points[0].id, entity_b: lines[0].id, value: 1.0 }) });
		}

		// 1 circle or arc
		if ((circles.length === 1 || arcs.length === 1) && points.length === 0 && lines.length === 0) {
			const entity = circles[0] || arcs[0];
			result.push({ label: 'Radius', build: () => ({ type: 'Radius', entity: entity.id, value: entity.radius || 1.0 }) });
			result.push({ label: 'Diameter', build: () => ({ type: 'Diameter', entity: entity.id, value: (entity.radius || 1.0) * 2 }) });
		}

		// 1 line + 1 arc
		if (lines.length === 1 && arcs.length === 1 && points.length === 0 && circles.length === 0) {
			result.push({ label: 'Tangent', build: () => ({ type: 'Tangent', line: lines[0].id, curve: arcs[0].id }) });
		}

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
