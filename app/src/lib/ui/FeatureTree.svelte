<script>
	import { getFeatureTree } from '$lib/engine/store.svelte.js';

	let tree = $derived(getFeatureTree());
</script>

<div class="feature-tree">
	<div class="panel-header">Features</div>
	<div class="tree-content">
		{#if tree.features.length === 0}
			<div class="empty-state">No features yet</div>
		{:else}
			{#each tree.features as feature, i}
				<div
					class="tree-item"
					class:suppressed={feature.suppressed}
					class:after-rollback={tree.active_index !== null && i > tree.active_index}
				>
					<span class="tree-icon">
						{#if feature.operation?.type === 'Sketch'}
							&#9998;
						{:else if feature.operation?.type === 'Extrude'}
							&#9647;
						{:else}
							&#8226;
						{/if}
					</span>
					<span class="tree-label">{feature.name}</span>
				</div>
			{/each}
		{/if}
	</div>
</div>

<style>
	.feature-tree {
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

	.tree-content {
		flex: 1;
		padding: 4px 0;
	}

	.empty-state {
		padding: 16px 12px;
		color: var(--text-muted);
		font-style: italic;
		font-size: 12px;
	}

	.tree-item {
		display: flex;
		align-items: center;
		padding: 3px 12px;
		cursor: pointer;
		gap: 6px;
	}

	.tree-item:hover {
		background: var(--bg-hover);
	}

	.tree-item.suppressed,
	.tree-item.after-rollback {
		opacity: 0.4;
	}

	.tree-icon {
		width: 16px;
		text-align: center;
		font-size: 12px;
		color: var(--text-secondary);
	}

	.tree-label {
		font-size: 12px;
	}
</style>
