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
		DRAWING_PROJECTION_ANGLES,
		DRAWING_SHEET_SIZES,
		addDrawingSheet,
		addDrawingView,
		deleteDrawingSheet,
		deleteDrawingView,
		editDrawingSheet,
		editDrawingView,
		getDocumentTabs,
		getDrawing,
		getDrawingSheet,
		getDrawingStatus,
		setActiveDrawingSheetId,
		// D4d: what the sheet has selected, and the one door that changes it.
		getSheetSelection,
		setSheetSelection,
		editSheetAnnotation,
		deleteSheetAnnotation
	} from '$lib/engine/store.svelte.js';
	// The one copy on this side (D4b review): the panel, the detail caption and
	// the title block's `Scale` row must all read a scale the same way.
	import { scaleRatioLabel } from '$lib/drawings/sheet.js';

	let status = $derived(getDrawingStatus());
	let drawing = $derived(getDrawing());
	let sheets = $derived(drawing?.sheets ?? []);
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
	/** D4b: what a view OF A PARENT is — a projection, a cut, or a crop. */
	let derived = $state('projected');
	/** The cutting line and the crop disc, in the parent's own mm. */
	let cutFrom = $state([0, 0]);
	let cutTo = $state([0, 10]);
	let cutFlip = $state(false);
	let cropAt = $state([0, 0]);
	let cropRadius = $state(5);

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
		const parentName = () => views.find((v) => v.id === p.parent)?.name ?? '?';
		if (p?.type === 'ProjectedFrom') {
			return `${p.direction?.type ?? '?'} of ${parentName()}`;
		}
		// D4b. The letter is what a reader matches against the parent's own
		// cutting line, so it leads.
		if (p?.type === 'Section') return `section ${p.label ?? '?'} of ${parentName()}`;
		if (p?.type === 'Detail') return `detail ${p.label ?? '?'} of ${parentName()}`;
		return '?';
	}

	/** The panel's millimetres as the store's meters. */
	function mm(pair) {
		return [Number(pair[0]) / 1000, Number(pair[1]) / 1000];
	}

	async function add() {
		if (!sourceTab) return;
		await run(async () => {
			if (!parentView) {
				await addDrawingView(sourceTab, { view: namedView });
				return;
			}
			if (derived === 'section') {
				await addDrawingView(sourceTab, {
					parent: parentView,
					section: { from: mm(cutFrom), to: mm(cutTo), flip: cutFlip }
				});
				return;
			}
			if (derived === 'detail') {
				await addDrawingView(sourceTab, {
					parent: parentView,
					scale: 2,
					detail: { center: mm(cropAt), radius: Number(cropRadius) / 1000 }
				});
				return;
			}
			await addDrawingView(sourceTab, { parent: parentView, direction });
		});
	}

	// The first available source is the useful default: on a two-tab document
	// (one part, one drawing) it is the only choice there is.
	$effect(() => {
		if (!sourceTab && sources.length) sourceTab = sources[0].id;
	});

	// ── The selected annotation (D4d) ───────────────────────────────────
	// §8's "the panel shows its precision, tolerance (M1) and dual unit for
	// that selection". There is no `value` field and there cannot be: the
	// engine measures a dimension from the model on every rebuild and refuses
	// a literal, so the only things authorable here are how the measured
	// number is PRINTED and what a note or a datum says.
	let selection = $derived(getSheetSelection());
	let selectedView = $derived(
		selection ? (views.find((v) => v.id === selection.viewId) ?? null) : null
	);
	let selected = $derived(
		selection && selectedView ? (selectedView.annotations?.[selection.index] ?? null) : null
	);
	/** The MEASURED value, read from the layout — the only place it exists. */
	let selectedValue = $derived(
		selection && selectedView ? (selectedView.cache?.annotations?.[selection.index] ?? null) : null
	);

	/** The dual units offered: the ones `units.js` knows, plus "none". */
	const DUAL_UNITS = ['', 'mm', 'cm', 'm', 'in', 'ft'];

	async function changeSelected(changes) {
		if (!selection) return;
		await run(() => editSheetAnnotation(selection.viewId, selection.index, changes));
	}
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
			<!-- D4b: the sheet's own controls. The paper, the projection
			     standard (a DOCUMENT setting, which is why it is here and not
			     per view) and the title block. -->
			<div class="row-main">
				<select
					data-testid="dwg-sheet-pick"
					value={sheet?.id ?? ''}
					disabled={busy || sheets.length < 2}
					onchange={(e) => setActiveDrawingSheetId(e.currentTarget.value)}
				>
					{#each sheets as s, i (s.id)}
						<option value={s.id}>{s.name} ({i + 1}/{sheets.length})</option>
					{/each}
				</select>
				<button
					class="act"
					title="Add a sheet to this drawing"
					data-testid="dwg-sheet-add"
					disabled={busy}
					onclick={() => run(() => addDrawingSheet({}))}>+ sheet</button
				>
				<button
					class="act"
					title="Remove this sheet and the views on it (a drawing keeps at least one)"
					data-testid="dwg-sheet-remove"
					disabled={busy || sheets.length < 2}
					onclick={() => run(() => deleteDrawingSheet(sheet?.id))}>×</button
				>
			</div>
			<div class="row-main">
				<label title="Paper size">
					paper
					<select
						data-testid="dwg-sheet-size-input"
						value={sheet?.size?.type ?? 'A3'}
						disabled={busy}
						onchange={(e) =>
							run(() => editDrawingSheet({ sheetId: sheet?.id, size: e.currentTarget.value }))}
					>
						{#each DRAWING_SHEET_SIZES as size}
							<option value={size}>{size}</option>
						{/each}
					</select>
				</label>
				<label title="Portrait or landscape">
					<select
						data-testid="dwg-sheet-orientation"
						value={sheet?.orientation?.type ?? 'Landscape'}
						disabled={busy}
						onchange={(e) =>
							run(() =>
								editDrawingSheet({ sheetId: sheet?.id, orientation: e.currentTarget.value })
							)}
					>
						<option value="Landscape">landscape</option>
						<option value="Portrait">portrait</option>
					</select>
				</label>
			</div>
			<div class="row-main">
				<label
					title="Third angle (ISO/ASME default) places a view on the side it is viewed from, so the view to the right of its parent shows the right-hand side. First angle places it opposite."
				>
					projection
					<select
						data-testid="dwg-projection-angle"
						value={drawing?.projection_angle?.type ?? 'Third'}
						disabled={busy}
						onchange={(e) => run(() => editDrawingSheet({ projectionAngle: e.currentTarget.value }))}
					>
						{#each DRAWING_PROJECTION_ANGLES as angle}
							<option value={angle}>{angle.toLowerCase()} angle</option>
						{/each}
					</select>
				</label>
				<label title="Draw the title block in the frame's bottom-right corner">
					<input
						type="checkbox"
						data-testid="dwg-title-block"
						checked={sheet?.title_block?.show !== false}
						disabled={busy}
						onchange={(e) =>
							run(() =>
								editDrawingSheet({ sheetId: sheet?.id, titleBlock: e.currentTarget.checked })
							)}
					/> title block
				</label>
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
						<span class="meta" data-testid="dwg-view-scale-{i}">{scaleRatioLabel(view.scale)}</span>
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
						<!-- D4b: what a view OF a parent is. A projection takes
						     a side; a section takes the cutting line it is cut
						     along; a detail takes the disc it crops. -->
						<select data-testid="dwg-add-derived" bind:value={derived} disabled={busy}>
							<option value="projected">projected</option>
							<option value="section">section</option>
							<option value="detail">detail</option>
						</select>
						{#if derived === 'projected'}
							<select data-testid="dwg-add-direction" bind:value={direction} disabled={busy}>
								{#each DRAWING_PROJECTED_DIRECTIONS as d}
									<option value={d}>{d.toLowerCase()} of it</option>
								{/each}
							</select>
						{/if}
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
				{#if parentView && derived === 'section'}
					<div class="row-sub" data-testid="dwg-add-section">
						<span
							class="xyz"
							title="The cutting line's two ends, in the parent view's own plane: mm right and up from its origin"
						>
							cut
							{#each [0, 1] as k}
								<input
									class="num"
									type="number"
									step="1"
									data-testid="dwg-cut-from-{k}"
									value={cutFrom[k]}
									disabled={busy}
									onchange={(e) =>
										(cutFrom = k === 0
											? [Number(e.currentTarget.value), cutFrom[1]]
											: [cutFrom[0], Number(e.currentTarget.value)])}
								/>
							{/each}
							→
							{#each [0, 1] as k}
								<input
									class="num"
									type="number"
									step="1"
									data-testid="dwg-cut-to-{k}"
									value={cutTo[k]}
									disabled={busy}
									onchange={(e) =>
										(cutTo = k === 0
											? [Number(e.currentTarget.value), cutTo[1]]
											: [cutTo[0], Number(e.currentTarget.value)])}
								/>
							{/each}
							<span class="unit">mm</span>
						</span>
						<label title="Keep the other half — the arrows reverse, the line does not move">
							<input type="checkbox" data-testid="dwg-cut-flip" bind:checked={cutFlip} disabled={busy} />
							flip
						</label>
					</div>
				{/if}
				{#if parentView && derived === 'detail'}
					<div class="row-sub" data-testid="dwg-add-detail">
						<span
							class="xyz"
							title="The crop disc's centre and radius, in the parent view's own plane (mm). A detail is added at 2:1."
						>
							crop
							{#each [0, 1] as k}
								<input
									class="num"
									type="number"
									step="1"
									data-testid="dwg-crop-at-{k}"
									value={cropAt[k]}
									disabled={busy}
									onchange={(e) =>
										(cropAt = k === 0
											? [Number(e.currentTarget.value), cropAt[1]]
											: [cropAt[0], Number(e.currentTarget.value)])}
								/>
							{/each}
							r
							<input
								class="num"
								type="number"
								step="1"
								min="0.1"
								data-testid="dwg-crop-radius"
								bind:value={cropRadius}
								disabled={busy}
							/>
							<span class="unit">mm</span>
						</span>
					</div>
				{/if}
			</div>
			{#if !sources.length}
				<div class="row">
					<span class="meta">This document has no Part or Assembly tab to draw.</span>
				</div>
			{/if}
		</div>

		{#if selected}
			<!-- D4d: the sheet's selection. Shown here rather than on the paper
			     because a field floating over a drawing hides the drawing. -->
			<div class="section" data-testid="dwg-annotation">
				<div class="section-header">
					{selected.type === 'Dimension' ? (selected.kind?.type ?? 'Dimension') : selected.type}
					<span class="meta" data-testid="dwg-annotation-value">
						{#if selectedValue && Number.isFinite(selectedValue.value)}
							measured {selectedValue.value}
						{:else}
							not measured
						{/if}
					</span>
					<button
						class="act"
						title="Clear the selection"
						data-testid="dwg-annotation-clear"
						onclick={() => setSheetSelection(null)}>×</button
					>
				</div>
				{#if selected.type === 'Dimension'}
					<div class="row-main">
						<label title="Decimal places. Blank follows the document's own precision.">
							places
							<input
								class="num"
								type="number"
								min="0"
								max="6"
								step="1"
								data-testid="dwg-annotation-precision"
								value={selected.precision ?? ''}
								disabled={busy}
								onchange={(e) =>
									changeSelected({
										precision:
											e.currentTarget.value === '' ? null : Number(e.currentTarget.value)
									})}
							/>
						</label>
						<label
							title="A second unit printed in brackets. ASME Y14.5 §1.6.2 wants the conversion to keep the implied precision; a separate dual precision is M1's."
						>
							dual
							<select
								data-testid="dwg-annotation-dual"
								value={selected.dual_unit ?? ''}
								disabled={busy}
								onchange={(e) => changeSelected({ dualUnit: e.currentTarget.value })}
							>
								{#each DUAL_UNITS as u}
									<option value={u}>{u === '' ? 'none' : u}</option>
								{/each}
							</select>
						</label>
					</div>
					<div class="row-sub">
						<!-- M1 owns `Tolerance`; until it lands there is nothing to
						     show, and an empty control wired to nothing would be
						     worse than a missing one (the D4a `ViewStyle` call). -->
						<span class="meta">tolerance: M1</span>
					</div>
				{:else if selected.type === 'Note'}
					<div class="row-main">
						<input
							class="name"
							data-testid="dwg-annotation-text"
							value={selected.text ?? ''}
							disabled={busy}
							onchange={(e) => changeSelected({ text: e.currentTarget.value })}
						/>
					</div>
				{:else if selected.type === 'Datum'}
					<div class="row-main">
						<label title="The datum letter">
							label
							<input
								class="num"
								data-testid="dwg-annotation-label"
								value={selected.label ?? ''}
								disabled={busy}
								onchange={(e) => changeSelected({ label: e.currentTarget.value })}
							/>
						</label>
					</div>
				{/if}
				<div class="row-sub">
					<button
						class="act"
						title="Remove this annotation (Delete)"
						data-testid="dwg-annotation-remove"
						disabled={busy}
						onclick={() => run(() => deleteSheetAnnotation(selection.viewId, selection.index))}
						>delete</button
					>
				</div>
			</div>
		{/if}

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
