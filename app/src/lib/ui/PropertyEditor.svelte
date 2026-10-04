<script>
	import {
		getSelectedFeature,
		editFeature,
		isEngineReady,
		getSketchMode,
		getSnapSettings,
		updateSnapSettings
	} from '$lib/engine/store.svelte.js';

	let feature = $derived(getSelectedFeature());
	let ready = $derived(isEngineReady());

	/** @type {ReturnType<typeof setTimeout> | null} */
	let debounceTimer = null;

	/**
	 * Handle parameter change with debounce.
	 * @param {string} paramPath - dot-separated path into operation params
	 * @param {any} value
	 */
	function handleChange(paramPath, value, exprPath = null) {
		if (!feature || !ready) return;

		if (debounceTimer) clearTimeout(debounceTimer);
		debounceTimer = setTimeout(() => {
			// `$state.snapshot` FIRST: `feature` comes off the `$state` feature
			// tree, so `feature.operation` is a reactive Proxy and
			// `structuredClone` of a Proxy is a DataCloneError in V8. It threw
			// here on every edit, so `editFeature` below was never reached and
			// every number and checkbox in this panel was a silent no-op —
			// measured, typing 25 into an extrude's depth left it at 10 mm.
			// Nothing reported it: the throw is an unhandled rejection inside a
			// timeout callback, and the input keeps the typed value.
			const op = structuredClone($state.snapshot(feature.operation));
			setNestedValue(op, paramPath, value);
			// A plain numeric edit detaches any driving expression — otherwise
			// the next rebuild would silently re-evaluate the expression over
			// the typed value, which reads as an edit that did nothing (same
			// rule as sketch dimension edits).
			//
			// The twin travels on the FIELD (`getFields`), not in a list here:
			// this used to name `depth_expr` and `angle_expr` only, so the six
			// other twinned numbers reachable from this panel — a pipe's two
			// radii, both pattern counts, a circular pattern's angle and a
			// linear pattern's spacing — kept their expression and silently
			// reverted. A field added to `getFields` without its `expr` is now
			// one edit away from showing it.
			if (exprPath) setNestedValue(op, exprPath, null);
			editFeature(feature.id, op);
		}, 300);
	}

	function setNestedValue(obj, path, value) {
		const keys = path.split('.');
		let target = obj;
		for (let i = 0; i < keys.length - 1; i++) {
			target = target[keys[i]];
			if (!target) return;
		}
		target[keys[keys.length - 1]] = value;
	}

	/**
	 * How many bodies a pattern seeds from: a list has a length, and
	 * `{type: "All"}` means every live body at that point in the tree.
	 * @param {any} seeds
	 */
	function seedCount(seeds) {
		if (Array.isArray(seeds)) return seeds.length;
		return seeds?.type === 'All' ? 'all live' : 0;
	}

	/**
	 * Get display fields for an operation type.
	 */
	function getFields(operation) {
		if (!operation) return [];
		switch (operation.type) {
			case 'Extrude':
				return [
					{ key: 'params.depth', label: 'Depth', type: 'number', value: operation.params?.depth, expr: 'params.depth_expr' },
					{ key: 'params.symmetric', label: 'Symmetric', type: 'boolean', value: operation.params?.symmetric },
					{ key: 'params.cut', label: 'Cut', type: 'boolean', value: operation.params?.cut },
				];
			case 'Revolve':
				return [
					{ key: 'params.angle', label: 'Angle (°)', type: 'number', value: operation.params?.angle, expr: 'params.angle_expr' },
				];
			case 'Pipe':
				return [
					{ key: 'params.radius', label: 'Radius', type: 'number', value: operation.params?.radius, expr: 'params.radius_expr' },
					{ key: 'params.inner_radius', label: 'Bore radius', type: 'number', value: operation.params?.inner_radius, expr: 'params.inner_radius_expr' },
				];
			case 'Sweep':
				return [
					{ key: '_info', label: 'Path', type: 'info', value: operation.params?.path?.type === 'Sketch3d' ? '3D sketch chain' : `${operation.params?.path?.entity_ids?.length ?? 0} sketch entities` },
					{ key: '_info2', label: 'Combine', type: 'info', value: operation.params?.combine ?? 'NewBody' },
				];
			case 'Fillet':
				return [
					{ key: 'params.radius', label: 'Radius', type: 'number', value: operation.params?.radius },
				];
			case 'Chamfer':
				return [
					{ key: 'params.distance', label: 'Distance', type: 'number', value: operation.params?.distance },
				];
			case 'Shell':
				return [
					{ key: 'params.thickness', label: 'Thickness', type: 'number', value: operation.params?.thickness },
				];
			case 'PatternCircular':
				return [
					{ key: 'params.count', label: 'Count', type: 'number', value: operation.params?.count, expr: 'params.count_expr' },
					{ key: 'params.angle_deg', label: 'Angle (°)', type: 'number', value: operation.params?.angle_deg, expr: 'params.angle_expr' },
					{ key: '_info', label: 'Seeds', type: 'info', value: seedCount(operation.params?.seeds) },
					{ key: '_info2', label: 'Combine', type: 'info', value: operation.params?.combine?.type ?? 'NewBody' },
				];
			case 'PatternLinear':
				return [
					{ key: 'params.count', label: 'Count', type: 'number', value: operation.params?.count, expr: 'params.count_expr' },
					{ key: 'params.spacing', label: 'Spacing', type: 'number', value: operation.params?.spacing, expr: 'params.spacing_expr' },
					{ key: '_info', label: 'Seeds', type: 'info', value: seedCount(operation.params?.seeds) },
					{ key: '_info2', label: 'Combine', type: 'info', value: operation.params?.combine?.type ?? 'NewBody' },
				];
			case 'PatternMirror':
				return [
					{ key: '_info', label: 'Seeds', type: 'info', value: seedCount(operation.params?.seeds) },
					{ key: '_info2', label: 'Combine', type: 'info', value: operation.params?.combine?.type ?? 'NewBody' },
				];
			case 'Script': {
				// The node's arguments as the script sees them (A-M4): an
				// expression-driven one shows its expression; geometry
				// arguments show their kind. Double-click the node to edit.
				const args = operation.params?.args ?? {};
				const exprs = operation.params?.arg_exprs ?? {};
				const rows = [{ key: '_info', label: 'Entry', type: 'info', value: operation.params?.entry ?? 'feature' }];
				for (const [name, expr] of Object.entries(exprs)) {
					rows.push({ key: `_arg_${name}`, label: name, type: 'info', value: `= ${expr}` });
				}
				for (const [name, value] of Object.entries(args)) {
					if (name in exprs) continue;
					let shown;
					if (value && typeof value === 'object') {
						shown = value.kind?.type ? value.kind.type.toLowerCase() : Array.isArray(value.normal) ? `plane n=(${value.normal.map((c) => +(+c).toFixed(3)).join(', ')})` : 'object';
					} else {
						shown = String(value);
					}
					rows.push({ key: `_arg_${name}`, label: name, type: 'info', value: shown });
				}
				return rows;
			}
			case 'Sketch':
				return [
					{ key: '_info', label: 'Entities', type: 'info', value: operation.sketch?.entities?.length ?? 0 },
					{ key: '_info2', label: 'Constraints', type: 'info', value: operation.sketch?.constraints?.length ?? 0 },
				];
			default:
				return [];
		}
	}

	let fields = $derived(feature ? getFields(feature.operation) : []);
	let inSketch = $derived(getSketchMode()?.active ?? false);
	let snap = $derived(getSnapSettings());
</script>

<div class="property-editor" data-testid="property-editor">
	<div class="panel-header">Properties</div>
	<div class="editor-content">
		{#if inSketch}
			<div class="section-header">Snap Settings</div>
			<div class="fields">
				<div class="field-row">
					<label class="field-label">Point snap (px)</label>
					<input
						class="field-input"
						type="number"
						min="1"
						max="30"
						step="1"
						data-testid="snap-coincidentPx"
						value={snap.coincidentPx}
						onchange={(e) => updateSnapSettings({ coincidentPx: parseInt(e.target.value) || 8 })}
					/>
				</div>
				<div class="field-row">
					<label class="field-label">Entity snap (px)</label>
					<input
						class="field-input"
						type="number"
						min="1"
						max="20"
						step="1"
						data-testid="snap-onEntityPx"
						value={snap.onEntityPx}
						onchange={(e) => updateSnapSettings({ onEntityPx: parseInt(e.target.value) || 5 })}
					/>
				</div>
				<div class="field-row">
					<label class="field-label">H/V angle (deg)</label>
					<input
						class="field-input"
						type="number"
						min="1"
						max="15"
						step="0.5"
						data-testid="snap-hvAngleDeg"
						value={snap.hvAngleDeg}
						onchange={(e) => updateSnapSettings({ hvAngleDeg: parseFloat(e.target.value) || 3 })}
					/>
				</div>
				<div class="field-row">
					<label class="field-label">Preview radius (px)</label>
					<input
						class="field-input"
						type="number"
						min="0"
						max="100"
						step="5"
						data-testid="snap-previewPx"
						value={snap.previewPx}
						onchange={(e) => updateSnapSettings({ previewPx: parseInt(e.target.value) || 30 })}
					/>
				</div>
			</div>
		{/if}

		{#if !feature}
			<div class="empty-state">{inSketch ? '' : 'Select a feature to edit its properties'}</div>
		{:else}
			<div class="feature-header">
				<span class="feature-type" data-testid="prop-feature-type">{feature.operation?.type ?? 'Unknown'}</span>
				<span class="feature-name" data-testid="prop-feature-name">{feature.name}</span>
			</div>

			{#if fields.length === 0}
				<div class="empty-state">No editable parameters</div>
			{:else}
				<div class="fields">
					{#each fields as field (field.key)}
						<div class="field-row">
							<label class="field-label">{field.label}</label>
							{#if field.type === 'number'}
								<input
									class="field-input"
									type="number"
									step="any"
									data-testid="prop-input-{field.key}"
									value={field.value}
									disabled={!ready}
									onchange={(e) => handleChange(field.key, parseFloat(e.target.value), field.expr)}
								/>
							{:else if field.type === 'boolean'}
								<input
									class="field-checkbox"
									type="checkbox"
									data-testid="prop-input-{field.key}"
									checked={field.value}
									disabled={!ready}
									onchange={(e) => handleChange(field.key, e.target.checked)}
								/>
							{:else if field.type === 'info'}
								<span class="field-info">{field.value}</span>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
</div>

<style>
	.property-editor {
		height: 100%;
		background: var(--bg-secondary);
		display: flex;
		flex-direction: column;
	}

	.panel-header {
		padding: 6px 12px;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--text-secondary);
		border-bottom: 1px solid var(--border-color);
		background: var(--bg-tertiary);
	}

	.editor-content {
		flex: 1;
		padding: 8px;
		overflow-y: auto;
	}

	.empty-state {
		padding: 16px 4px;
		color: var(--text-muted);
		font-style: italic;
		font-size: 12px;
	}

	.section-header {
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--text-secondary);
		padding-bottom: 6px;
		margin-bottom: 6px;
		border-bottom: 1px solid var(--border-color);
	}

	.feature-header {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-bottom: 8px;
		margin-bottom: 8px;
		border-bottom: 1px solid var(--border-color);
	}

	.feature-type {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--accent);
	}

	.feature-name {
		font-size: 13px;
		font-weight: 600;
	}

	.fields {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.field-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.field-label {
		font-size: 12px;
		color: var(--text-secondary);
		flex-shrink: 0;
	}

	.field-input {
		width: 80px;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		color: var(--text-primary);
		font-size: 12px;
		padding: 3px 6px;
		border-radius: 3px;
		outline: none;
		text-align: right;
	}

	.field-input:focus {
		border-color: var(--accent);
	}

	.field-input:disabled {
		opacity: 0.5;
	}

	.field-checkbox {
		accent-color: var(--accent);
	}

	.field-info {
		font-size: 12px;
		color: var(--text-muted);
	}
</style>
