<script>
	import {
		getStatusMessage,
		isEngineReady,
		getLastError,
		getSelectedFeature,
		getSelectedRefs,
		getSketchMode,
		getActiveTool,
		getRebuildTime
	} from '$lib/engine/store.svelte.js';

	let error = $derived(getLastError());
	let selectedFeature = $derived(getSelectedFeature());
	let selectedRefs = $derived(getSelectedRefs());
	let inSketch = $derived(getSketchMode()?.active ?? false);
	let tool = $derived(getActiveTool());
	let rebuildMs = $derived(getRebuildTime());

	let selectionText = $derived.by(() => {
		if (selectedRefs.length > 0) {
			return `${selectedRefs.length} ${selectedRefs.length === 1 ? 'entity' : 'entities'} selected`;
		}
		if (selectedFeature) {
			return selectedFeature.name;
		}
		return '';
	});

	let modeText = $derived(inSketch ? `Sketch Mode \u2022 Tool: ${tool}` : '');
</script>

<div class="statusbar" class:error={!!error}>
	<div class="status-left">
		<span class="status-text">{getStatusMessage()}</span>
		{#if modeText}
			<span class="status-sep">\u2502</span>
			<span class="status-mode">{modeText}</span>
		{/if}
	</div>
	<div class="status-right">
		{#if selectionText}
			<span class="status-selection">{selectionText}</span>
			<span class="status-sep">\u2502</span>
		{/if}
		{#if rebuildMs > 0}
			<span class="status-rebuild">Rebuild: {rebuildMs.toFixed(0)}ms</span>
			<span class="status-sep">\u2502</span>
		{/if}
		{#if isEngineReady()}
			<span class="status-engine">WASM Active</span>
		{/if}
	</div>
</div>

<style>
	.statusbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: 100%;
		background: var(--accent);
		padding: 0 8px;
		font-size: 11px;
		color: white;
		gap: 8px;
	}

	.statusbar.error {
		background: var(--error);
	}

	.status-left, .status-right {
		display: flex;
		align-items: center;
		gap: 6px;
		overflow: hidden;
	}

	.status-left {
		flex: 1;
		min-width: 0;
	}

	.status-right {
		flex-shrink: 0;
	}

	.status-text {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.status-sep {
		opacity: 0.4;
	}

	.status-mode {
		opacity: 0.9;
	}

	.status-selection, .status-rebuild {
		opacity: 0.8;
	}

	.status-engine {
		opacity: 0.7;
	}
</style>
