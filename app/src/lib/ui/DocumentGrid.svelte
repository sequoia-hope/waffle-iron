<script>
	import DocumentCard from './DocumentCard.svelte';

	let { documents = [], onselect } = $props();
</script>

{#if documents.length === 0}
	<div class="empty-state" data-testid="empty-state">
		<svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" opacity="0.4">
			<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
		</svg>
		<p>No documents yet</p>
		<p class="hint">Create your first document to get started</p>
	</div>
{:else}
	<div class="document-grid" data-testid="document-grid">
		{#each documents as doc, i (doc.id)}
			<DocumentCard {doc} index={i} onclick={onselect} />
		{/each}
	</div>
{/if}

<style>
	.document-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: 16px;
		padding: 24px 32px;
	}

	.empty-state {
		text-align: center;
		padding: 64px 24px;
		color: var(--text-secondary, #a6adc8);
	}

	.empty-state p {
		margin: 12px 0 0;
		font-size: 16px;
	}

	.empty-state .hint {
		font-size: 13px;
		color: var(--text-muted, #6c7086);
	}
</style>
