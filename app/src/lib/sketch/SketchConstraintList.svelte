<!--
  The active sketch's constraints as a list (user request 2026-10-30: "a ui
  to show sketch constraints and select and delete them, and over
  constraints should make them visible automatically").

  One row per constraint, in sketch order: its picture, what it is, what it
  binds and any value. A row selects the same constraint a badge click does
  (`selectedConstraintIndex`), so the canvas badge highlights with it, and
  Delete / the row's × removes it as one undoable step. When the solver
  reports a conflict, the conflicting rows turn red and scroll into view by
  themselves: the user should never have to hunt for what to delete.
-->
<script>
	import {
		getSketchConstraints,
		getSketchEntities,
		getFailedConstraintIndices,
		getSelectedConstraintIndex,
		setSelectedConstraintIndex,
		removeSketchConstraint,
		getDocumentDisplayUnit,
		getMobileLayout,
		getMobileActivePanel,
		toggleMobilePanel,
		getSketchDragActive
	} from '$lib/engine/store.svelte.js';
	import { formatWithUnit } from '$lib/units.js';
	import Icon from '$lib/ui/icons/Icon.svelte';

	let constraints = $derived(getSketchConstraints());
	let entities = $derived(getSketchEntities());
	let failed = $derived(getFailedConstraintIndices());
	let selected = $derived(getSelectedConstraintIndex());
	let unit = $derived(getDocumentDisplayUnit());

	/** type → [display name, icon] */
	const KINDS = {
		Horizontal: ['Horizontal', 'constraint-horizontal'],
		HorizontalPoints: ['Horizontal', 'constraint-horizontal'],
		Vertical: ['Vertical', 'constraint-vertical'],
		VerticalPoints: ['Vertical', 'constraint-vertical'],
		Coincident: ['Coincident', 'constraint-coincident'],
		Perpendicular: ['Perpendicular', 'constraint-perpendicular'],
		Parallel: ['Parallel', 'constraint-parallel'],
		Equal: ['Equal', 'constraint-equal'],
		EqualRadius: ['Equal radius', 'constraint-equal-radius'],
		Tangent: ['Tangent', 'constraint-tangent'],
		Midpoint: ['Midpoint', 'constraint-midpoint'],
		WhereDragged: ['Fixed', 'constraint-fix'],
		Pinned: ['Fixed', 'constraint-fix'],
		Angle: ['Angle', 'constraint-angle'],
		Symmetric: ['Symmetric', 'constraint-symmetric'],
		SymmetricH: ['Symmetric (horizontal)', 'constraint-symmetric-h'],
		SymmetricV: ['Symmetric (vertical)', 'constraint-symmetric-v'],
		OnEntity: ['Point on', 'constraint-point-on-line'],
		PointOnCircle: ['Point on circle', 'constraint-point-on-circle'],
		Distance: ['Distance', 'dim-distance'],
		HDistance: ['Horizontal distance', 'constraint-h-distance'],
		VDistance: ['Vertical distance', 'constraint-v-distance'],
		PointLineDistance: ['Point–line distance', 'dim-pointline'],
		Radius: ['Radius', 'dim-radius'],
		Diameter: ['Diameter', 'dim-diameter'],
		LengthRatio: ['Length ratio', 'constraint-length-ratio']
	};

	const REF_FIELDS = ['entity', 'entity_a', 'entity_b', 'line', 'line_a', 'line_b', 'curve', 'point', 'point_a', 'point_b'];

	/** "Line 5, Point 2" — what the constraint binds. */
	function refsText(c, byId) {
		const ids = [];
		for (const f of REF_FIELDS) if (c[f] != null && !ids.includes(c[f])) ids.push(c[f]);
		return ids.map((id) => `${byId.get(id)?.type ?? 'Entity'} ${id}`).join(', ');
	}

	function valueText(c) {
		if (c.type === 'Angle') return c.value_degrees != null ? `${Number(c.value_degrees).toFixed(1)}°` : '';
		if (c.type === 'LengthRatio') return c.value != null ? `${Number(c.value).toFixed(3)}` : '';
		if (typeof c.value === 'number') {
			const v = formatWithUnit(c.value, unit);
			return c.reference ? `${v} (REF)` : v;
		}
		return '';
	}

	let rows = $derived.by(() => {
		const byId = new Map(entities.map((e) => [e.id, e]));
		return constraints
			.map((c, index) => {
				// A live drag pin is the drag itself, not a constraint the user made.
				if (c._isDrag) return null;
				const [name, icon] = KINDS[c.type] ?? [c.type, 'info'];
				return { index, name, icon, refs: refsText(c, byId), value: valueText(c), failed: failed.has(index) };
			})
			.filter(Boolean);
	});

	let failedCount = $derived(rows.filter((r) => r.failed).length);

	/** @type {HTMLElement | null} */
	let listEl = $state(null);

	// Over-constraint shows itself: when conflicts appear (or change), bring
	// the first conflicting row into view without the user asking. On a phone
	// the Properties panel is a closed drawer, so it opens too, but never in
	// the middle of a drag (it would cover the canvas under the finger): a
	// conflict found while dragging is revealed on release.
	let lastFailedKey = '';
	let dragging = $derived(getSketchDragActive());
	$effect(() => {
		const key = [...failed].sort((a, b) => a - b).join(',');
		if (key === lastFailedKey) return;
		if (key && dragging) return;
		lastFailedKey = key;
		if (!key) return;
		if (getMobileLayout() && getMobileActivePanel() !== 'right') toggleMobilePanel('right');
		queueMicrotask(() => {
			listEl?.querySelector('.constraint-row.failed')?.scrollIntoView({ block: 'nearest' });
		});
	});

	function select(index) {
		setSelectedConstraintIndex(selected === index ? null : index);
	}

	function remove(index) {
		if (selected === index) setSelectedConstraintIndex(null);
		removeSketchConstraint(index);
	}
</script>

<div class="constraints-header" data-testid="constraint-list-header">
	Constraints
	<span class="count">{rows.length}</span>
	{#if failedCount > 0}
		<span class="conflicts" data-testid="constraint-list-conflicts">{failedCount} conflicting</span>
	{/if}
</div>
<div class="constraint-list" data-testid="constraint-list" bind:this={listEl}>
	{#if rows.length === 0}
		<div class="empty">No constraints yet</div>
	{/if}
	{#each rows as r (r.index)}
		<div
			class="constraint-row"
			class:failed={r.failed}
			class:selected={selected === r.index}
			data-testid="constraint-row-{r.index}"
			data-failed={r.failed ? 'true' : 'false'}
			role="option"
			aria-selected={selected === r.index}
			tabindex="0"
			onclick={() => select(r.index)}
			onkeydown={(e) => {
				if (e.key === 'Enter' || e.key === ' ') {
					e.preventDefault();
					select(r.index);
				} else if (e.key === 'Delete' || e.key === 'Backspace') {
					e.preventDefault();
					e.stopPropagation();
					remove(r.index);
				}
			}}
		>
			<Icon name={r.icon} />
			<span class="name">{r.name}</span>
			<span class="refs" title={r.refs}>{r.refs}</span>
			{#if r.value}<span class="value">{r.value}</span>{/if}
			<button
				class="delete"
				title="Delete constraint"
				aria-label="Delete {r.name} constraint"
				data-testid="constraint-delete-{r.index}"
				onclick={(e) => {
					e.stopPropagation();
					remove(r.index);
				}}><Icon name="trash-2" size={14} /></button
			>
		</div>
	{/each}
</div>

<style>
	/* Same look as the Properties panel's own section headers. */
	.constraints-header {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--text-secondary);
		padding-bottom: 6px;
		margin-bottom: 6px;
		border-bottom: 1px solid var(--border-color);
	}
	.count {
		color: var(--text-muted);
		font-weight: normal;
	}
	.conflicts {
		margin-left: auto;
		color: var(--sketch-overconstrained);
		font-weight: 600;
	}
	.constraint-list {
		max-height: 240px;
		overflow-y: auto;
		margin-bottom: 8px;
	}
	.empty {
		color: var(--text-muted);
		font-size: 11px;
		padding: 4px 8px;
	}
	.constraint-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 6px;
		border-radius: 3px;
		font-size: 12px;
		color: var(--text-primary);
		cursor: pointer;
		border: 1px solid transparent;
	}
	.constraint-row:hover {
		background: var(--bg-hover);
	}
	.constraint-row.selected {
		border-color: var(--sketch-selected);
		color: var(--sketch-selected);
	}
	.constraint-row.failed {
		color: var(--sketch-overconstrained);
		background: color-mix(in srgb, var(--sketch-overconstrained) 12%, transparent);
	}
	.name {
		flex: none;
	}
	.refs {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-secondary);
		font-size: 11px;
	}
	.constraint-row.failed .refs {
		color: inherit;
	}
	.value {
		flex: none;
		font-family: monospace;
		font-size: 11px;
	}
	.delete {
		flex: none;
		display: inline-flex;
		background: none;
		border: none;
		padding: 2px;
		color: var(--text-muted);
		cursor: pointer;
		border-radius: 3px;
	}
	.delete:hover {
		color: var(--sketch-overconstrained);
		background: var(--bg-hover);
	}
</style>
