<script>
</script>

<div class="property-editor">
	<div class="panel-header">Properties</div>
	<div class="editor-content">
		<div class="empty-state">Select a feature to edit its properties</div>
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
	}

	.empty-state {
		padding: 16px 4px;
		color: var(--text-muted);
		font-style: italic;
		font-size: 12px;
	}
</style>
