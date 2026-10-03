<script>
	/**
	 * The drawing sidebar (`specs/drawings_and_mbd.md` §8, D4a): the sheet, its
	 * views and what the evaluation had to say — where `FeatureTree` sits on a
	 * Part tab, the way `AssemblyPanel` replaces it on an Assembly tab.
	 *
	 * Reads the store, takes no props (the `AssemblyPanel` contract): the open
	 * Drawing tab's evaluation is store state, and the panel renders it.
	 *
	 * Every edit goes through the store's targeted `DrawingEdit` path, which
	 * names the one change and re-evaluates the tab — so the sheet follows in
	 * one step, and a drawing's `u64` anchor ids never make the round trip
	 * through JavaScript that would round them (see `sendDrawingEdit`).
	 */
	import {
		DRAWING_NAMED_VIEWS,
		DRAWING_PROJECTED_DIRECTIONS,
		addDrawingView,
		deleteDrawingView,
		editDrawingView,
		getDocumentTabs,
		getDrawingSheet,
		getDrawingStatus
	} from '$lib/engine/store.svelte.js';

	let status = $derived(getDrawingStatus());
	let sheet = $derived(getDrawingSheet(null));
	let views = $derived(sheet?.views ?? []);
	/** The tabs a view can draw: this document's Parts and Assemblies. */
	let sources = $derived(
		getDocumentTabs().filter((t) => t.kind?.type === 'Part' || t.kind?.type === 'Assembly')
	);

	let busy = $state(false);
	/** Which view's details are open. Six numbers per view make the list
	 *  unreadable with them all showing — the `AssemblyPanel` call. */
	let open = $state(new Set());
	/** The add-view form's own state. */
	let sourceTab = $state('');
	let namedView = $state('Front');
	let parentView = $state('');
	let direction = $state('Right');

	function toggle(id) {
		const next = new Set(open);
		if (!next.delete(id)) next.add(id);
		open = next;
	}

	async function run(fn) {
		if (busy) return;
		busy = true;
		try {
			await fn();
		} finally {
			busy = false;
		}
	}

	function tabName(id) {
		return getDocumentTabs().find((t) => t.id === id)?.name ?? id;
	}

	/** A view's projection, as one short label. */
	function projectionLabel(view) {
		const p = view?.projection;
		if (p?.type === 'Named') return p.view?.type ?? '?';
		if (p?.type === 'Custom') return 'custom';
		if (p?.type === 'ProjectedFrom') {
			const parent = views.find((v) => v.id === p.parent);
			return `${p.direction?.type ?? '?'} of ${parent?.name ?? '?'}`;
		}
		return '?';
	}

	/** `1:2`, `2:1`, `1:1` — the ratio a drafter reads, from the number. */
	function scaleLabel(scale) {
		const s = Number(scale);
		if (!Number.isFinite(s) || s <= 0) return '—';
		if (Math.abs(s - 1) < 1e-9) return '1:1';
		return s < 1 ? `1:${round(1 / s)}` : `${round(s)}:1`;
	}

	function round(x) {
		return Math.abs(x - Math.round(x)) < 1e-6 ? String(Math.round(x)) : x.toFixed(2);
	}

	async function add() {
		if (!sourceTab) return;
		await run(async () => {
			if (parentView) {
				await addDrawingView(sourceTab, { parent: parentView, direction });
			} else {
				await addDrawingView(sourceTab, { view: namedView });
			}
		});
	}

	// The first available source is the useful default: on a two-tab document
	// (one part, one drawing) it is the only choice there is.
	$effect(() => {
		if (!sourceTab && sources.length) sourceTab = sources[0].id;
	});
</script>

{#if status}
	<div class="drawing-panel" data-testid="drawing-panel">
		<div class="section">
			<div class="section-header">
				{sheet?.name ?? 'Sheet'}
				<span class="meta" data-testid="dwg-sheet-size">
					{sheet?.size?.type ?? '?'} {sheet?.orientation?.type === 'Portrait' ? 'portrait' : 'landscape'}
				</span>
			</div>
		</div>

		<div class="section">
			<div class="section-header">Views ({views.length})</div>
			{#each views as view, i (view.id)}
				<div class="row" data-testid="dwg-view-{i}">
					<div class="row-main">
						<input
							class="name"
							value={view.name}
							data-testid="dwg-view-name-{i}"
							disabled={busy}
							onchange={(e) => run(() => editDrawingView(view.id, { name: e.currentTarget.value }))}
						/>
						<span class="meta" data-testid="dwg-view-projection-{i}">{projectionLabel(view)}</span>
						<span class="meta" data-testid="dwg-view-scale-{i}">{scaleLabel(view.scale)}</span>
						<button
							class="act"
							title={open.has(view.id) ? 'Hide details' : 'Show scale, placement and style'}
							data-testid="dwg-view-toggle-{i}"
							onclick={() => toggle(view.id)}
						>
							{open.has(view.id) ? '▾' : '▸'} edit
						</button>
						<button
							class="act"
							title="Remove this view (and any view projected from it)"
							data-testid="dwg-view-remove-{i}"
							disabled={busy}
							onclick={() => run(() => deleteDrawingView(view.id))}>×</button
						>
					</div>
					<div class="row-sub">
						<span class="meta" data-testid="dwg-view-source-{i}">{tabName(view.source?.tab_id)}</span>
						<span class="meta" data-testid="dwg-view-curves-{i}">
							{view.cache?.curves?.length ?? 0} curves · {view.annotations?.length ?? 0} ann
						</span>
					</div>
					{#if open.has(view.id)}
						<div class="row-sub" data-testid="dwg-view-detail-{i}">
							<label title="Paper length per model length: 1 is 1:1, 0.5 is 1:2">
								scale
								<input
									class="num"
									type="number"
									step="0.1"
									min="0.01"
									data-testid="dwg-view-scale-input-{i}"
									value={view.scale}
									disabled={busy}
									onchange={(e) => run(() => editDrawingView(view.id, { scale: Number(e.currentTarget.value) }))}
								/>
							</label>
							<span class="xyz" title="Position on the sheet, mm from the bottom-left corner">
								{#each [0, 1] as k}
									<input
										class="num"
										type="number"
										step="5"
										data-testid="dwg-view-place-{k}-{i}"
										value={view.placement_mm?.[k] ?? 0}
										disabled={busy}
										onchange={(e) =>
											run(() =>
												editDrawingView(view.id, {
													placementMm: k === 0
														? [Number(e.currentTarget.value), view.placement_mm?.[1] ?? 0]
														: [view.placement_mm?.[0] ?? 0, Number(e.currentTarget.value)]
												})
											)}
									/>
								{/each}
								<span class="unit">mm</span>
							</span>
						</div>
						<div class="row-sub">
							<label title="Draw the edges the part hides (D1c)">
								<input
									type="checkbox"
									data-testid="dwg-view-hidden-{i}"
									checked={view.style?.hidden_lines !== false}
									disabled={busy}
									onchange={(e) => run(() => editDrawingView(view.id, { hiddenLines: e.currentTarget.checked }))}
								/> hidden lines
							</label>
							<label title="Draw curved faces' outlines (D1b)">
								<input
									type="checkbox"
									data-testid="dwg-view-silhouettes-{i}"
									checked={view.style?.silhouettes !== false}
									disabled={busy}
									onchange={(e) => run(() => editDrawingView(view.id, { silhouettes: e.currentTarget.checked }))}
								/> silhouettes
							</label>
						</div>
					{/if}
				</div>
			{/each}

			<div class="row add">
				<div class="row-main">
					<select data-testid="dwg-add-source" bind:value={sourceTab} disabled={busy || !sources.length}>
						{#each sources as t (t.id)}
							<option value={t.id}>{t.name}</option>
						{/each}
					</select>
					<select data-testid="dwg-add-parent" bind:value={parentView} disabled={busy}>
						<option value="">a named view</option>
						{#each views as v (v.id)}
							<option value={v.id}>projected from {v.name}</option>
						{/each}
					</select>
				</div>
				<div class="row-main">
					{#if parentView}
						<select data-testid="dwg-add-direction" bind:value={direction} disabled={busy}>
							{#each DRAWING_PROJECTED_DIRECTIONS as d}
								<option value={d}>{d.toLowerCase()} of it</option>
							{/each}
						</select>
					{:else}
						<select data-testid="dwg-add-view" bind:value={namedView} disabled={busy}>
							{#each DRAWING_NAMED_VIEWS as v}
								<option value={v}>{v.toLowerCase()}</option>
							{/each}
						</select>
					{/if}
					<button
						class="act primary"
						data-testid="dwg-add-view-button"
						disabled={busy || !sourceTab}
						onclick={add}>+ view</button
					>
				</div>
			</div>
			{#if !sources.length}
				<div class="row">
					<span class="meta">This document has no Part or Assembly tab to draw.</span>
				</div>
			{/if}
		</div>

		{#if Object.keys(status.declines ?? {}).length}
			<div class="section">
				<div class="section-header" title="What the projection declined to decide (D1c): every counter but cross_body is a line the drawing does not carry">
					Declined
				</div>
				<div class="row">
					{#each Object.entries(status.declines) as [name, count]}
						<span class="meta" data-testid="dwg-decline-{name}">{name} {count}</span>
					{/each}
				</div>
			</div>
		{/if}

		{#if status.errors?.length || status.warnings?.length}
			<div class="section status">
				{#each status.errors ?? [] as e}
					<div class="err" data-testid="dwg-error">{e}</div>
				{/each}
				{#each status.warnings ?? [] as w}
					<div class="warn" data-testid="dwg-warning">{w}</div>
				{/each}
			</div>
		{/if}
	</div>
{:else}
	<div class="drawing-panel" data-testid="drawing-panel">
		<div class="row"><span class="meta">The drawing is not evaluated yet.</span></div>
	</div>
{/if}

<style>
	/* Mirrors AssemblyPanel: the parent panel scrolls (`.left-panel`), each
	   row wraps, and the detail fields hide behind a per-row disclosure —
	   which is what keeps this inside its box at every width. */
	.drawing-panel {
		font-size: 12px;
	}

	.section {
		border-bottom: 1px solid var(--border-color);
		padding: 6px 8px;
	}

	.section-header {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 4px;
	}

	.row {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 3px 0;
	}

	.row-main,
	.row-sub {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.name {
		flex: 1 1 80px;
		min-width: 60px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		padding: 1px 4px;
		font-size: 12px;
	}

	.meta {
		color: var(--text-secondary);
		font-size: 11px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 100%;
	}

	.act {
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		font-size: 11px;
		padding: 1px 5px;
		cursor: pointer;
	}

	.act.primary {
		border-color: var(--color-accent, var(--border-color));
	}

	.act:disabled {
		opacity: 0.5;
		cursor: default;
	}

	select {
		flex: 1 1 80px;
		min-width: 60px;
		max-width: 100%;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		font-size: 11px;
		padding: 1px 2px;
	}

	.num {
		width: 56px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		font-size: 11px;
		padding: 1px 3px;
	}

	.xyz {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		flex-wrap: wrap;
	}

	.unit {
		color: var(--text-secondary);
		font-size: 11px;
	}

	.status .err,
	.status .warn {
		font-size: 11px;
		overflow-wrap: anywhere;
	}

	.status .err {
		color: var(--color-error, #c33);
	}

	.status .warn {
		color: var(--color-warning, #b80);
	}

	label {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		color: var(--text-secondary);
		font-size: 11px;
	}
</style>
