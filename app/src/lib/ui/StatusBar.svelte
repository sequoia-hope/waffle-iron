<script>
	import { getStatusMessage, isEngineReady, getLastError } from '$lib/engine/store.svelte.js';
</script>

<div class="statusbar" class:error={!!getLastError()}>
	<span class="status-text">{getStatusMessage()}</span>
	<span class="status-right">
		{#if isEngineReady()}
			WASM Engine Active
		{/if}
	</span>
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
	}

	.statusbar.error {
		background: var(--error);
	}

	.status-text {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.status-right {
		opacity: 0.8;
		flex-shrink: 0;
	}
</style>
