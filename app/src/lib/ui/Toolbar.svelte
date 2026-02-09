<script>
	import { isEngineReady } from '$lib/engine/store.svelte.js';
</script>

<div class="toolbar">
	<div class="toolbar-brand">Waffle Iron</div>
	<div class="toolbar-actions">
		<button class="toolbar-btn" disabled={!isEngineReady()} title="New Sketch">Sketch</button>
		<button class="toolbar-btn" disabled={!isEngineReady()} title="Extrude">Extrude</button>
		<button class="toolbar-btn" disabled={!isEngineReady()} title="Fillet">Fillet</button>
		<button class="toolbar-btn" disabled={!isEngineReady()} title="Chamfer">Chamfer</button>
	</div>
	<div class="toolbar-spacer"></div>
	<div class="toolbar-status">
		{#if isEngineReady()}
			<span class="status-dot ready"></span>
		{:else}
			<span class="status-dot loading"></span>
		{/if}
	</div>
</div>

<style>
	.toolbar {
		display: flex;
		align-items: center;
		height: 100%;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-color);
		padding: 0 8px;
		gap: 8px;
	}

	.toolbar-brand {
		font-weight: 600;
		font-size: 14px;
		color: var(--text-primary);
		padding-right: 12px;
		border-right: 1px solid var(--border-color);
	}

	.toolbar-actions {
		display: flex;
		gap: 2px;
	}

	.toolbar-btn {
		background: transparent;
		border: 1px solid transparent;
		color: var(--text-primary);
		padding: 4px 10px;
		border-radius: 3px;
		cursor: pointer;
		font-size: 12px;
	}

	.toolbar-btn:hover:not(:disabled) {
		background: var(--bg-hover);
		border-color: var(--border-color);
	}

	.toolbar-btn:disabled {
		color: var(--text-muted);
		cursor: default;
	}

	.toolbar-spacer {
		flex: 1;
	}

	.status-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.status-dot.ready {
		background: var(--success);
	}

	.status-dot.loading {
		background: var(--warning);
		animation: pulse 1s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.3; }
	}
</style>
