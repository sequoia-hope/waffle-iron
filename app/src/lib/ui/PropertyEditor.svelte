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
	function handleChange(paramPath, value) {
		if (!feature || !ready) return;

		if (debounceTimer) clearTimeout(debounceTimer);
		debounceTimer = setTimeout(() => {
			const op = structuredClone(feature.operation);
			setNestedValue(op, paramPath, value);
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
	 * Get display fields for an operation type.
	 */
	function getFields(operation) {
		if (!operation) return [];
		switch (operation.type) {
			case 'Extrude':
				return [
					{ key: 'params.depth', label: 'Depth', type: 'number', value: operation.params?.depth },
					{ key: 'params.symmetric', label: 'Symmetric', type: 'boolean', value: operation.params?.symmetric },
					{ key: 'params.cut', label: 'Cut', type: 'boolean', value: operation.params?.cut },
				];
			case 'Revolve':
				return [
					{ key: 'params.angle', label: 'Angle (rad)', type: 'number', value: operation.params?.angle },
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

<div class="property-editor">
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
						value={snap.hvAngleDeg}
						onchange={(e) => updateSnapSettings({ hvAngleDeg: parseFloat(e.target.value) || 3 })}
					/>
				</div>
			</div>
		{/if}

		{#if !feature}
			<div class="empty-state">{inSketch ? '' : 'Select a feature to edit its properties'}</div>
		{:else}
			<div class="feature-header">
				<span class="feature-type">{feature.operation?.type ?? 'Unknown'}</span>
				<span class="feature-name">{feature.name}</span>
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
									value={field.value}
									disabled={!ready}
									onchange={(e) => handleChange(field.key, parseFloat(e.target.value))}
								/>
							{:else if field.type === 'boolean'}
								<input
									class="field-checkbox"
									type="checkbox"
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
