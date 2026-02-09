<script>
	import {
		getFeatureTree,
		getSelectedFeatureId,
		selectFeature,
		deleteFeature,
		suppressFeature,
		setRollbackIndex,
		reorderFeature,
		renameFeature,
		send,
		isEngineReady
	} from '$lib/engine/store.svelte.js';

	let tree = $derived(getFeatureTree());
	let selectedId = $derived(getSelectedFeatureId());

	/** @type {{ x: number, y: number, featureId: string, featureName: string, suppressed: boolean } | null} */
	let contextMenu = $state(null);

	/** @type {{ featureId: string, value: string } | null} */
	let renaming = $state(null);

	// Drag-and-drop state
	/** @type {string | null} */
	let dragFeatureId = $state(null);
	/** @type {number | null} */
	let dropTargetIndex = $state(null);

	function handleClick(featureId) {
		selectFeature(featureId);
	}

	function handleDblClick(feature) {
		renaming = { featureId: feature.id, value: feature.name };
	}

	function handleContextMenu(e, feature) {
		e.preventDefault();
		contextMenu = {
			x: e.clientX,
			y: e.clientY,
			featureId: feature.id,
			featureName: feature.name,
			suppressed: feature.suppressed
		};
	}

	function closeContextMenu() {
		contextMenu = null;
	}

	function handleRename(e) {
		if (!renaming) return;
		if (e.key === 'Enter') {
			const trimmed = renaming.value.trim();
			if (trimmed) {
				renameFeature(renaming.featureId, trimmed);
			}
			renaming = null;
		} else if (e.key === 'Escape') {
			renaming = null;
		}
	}

	function handleRenameBlur() {
		if (!renaming) return;
		const trimmed = renaming.value.trim();
		if (trimmed) {
			renameFeature(renaming.featureId, trimmed);
		}
		renaming = null;
	}

	function handleDelete() {
		if (contextMenu) {
			deleteFeature(contextMenu.featureId);
			if (selectedId === contextMenu.featureId) selectFeature(null);
			closeContextMenu();
		}
	}

	function handleSuppress() {
		if (contextMenu) {
			suppressFeature(contextMenu.featureId, !contextMenu.suppressed);
			closeContextMenu();
		}
	}

	function featureIcon(opType) {
		switch (opType) {
			case 'Sketch': return '\u270E';
			case 'Extrude': return '\u25A7';
			case 'Revolve': return '\u21BB';
			case 'Fillet': return '\u25CF';
			case 'Chamfer': return '\u25C6';
			case 'Shell': return '\u25A1';
			case 'BooleanCombine': return '\u2229';
			default: return '\u2022';
		}
	}

	// -- Drag and drop --

	function handleDragStart(e, feature) {
		dragFeatureId = feature.id;
		e.dataTransfer.effectAllowed = 'move';
		e.dataTransfer.setData('text/plain', feature.id);
	}

	function handleDragOver(e, index) {
		e.preventDefault();
		e.dataTransfer.dropEffect = 'move';
		dropTargetIndex = index;
	}

	function handleDragLeave() {
		dropTargetIndex = null;
	}

	function handleDrop(e, targetIndex) {
		e.preventDefault();
		if (dragFeatureId) {
			reorderFeature(dragFeatureId, targetIndex);
		}
		dragFeatureId = null;
		dropTargetIndex = null;
	}

	function handleDragEnd() {
		dragFeatureId = null;
		dropTargetIndex = null;
	}

	// Rollback slider
	let rollbackValue = $derived(tree.active_index ?? tree.features.length);

	function handleRollback(e) {
		const val = parseInt(e.target.value);
		const index = val >= tree.features.length ? null : val;
		setRollbackIndex(index);
	}
</script>

<svelte:window onclick={closeContextMenu} />

<div class="feature-tree">
	<div class="panel-header">Features</div>
	<div class="tree-content">
		{#if tree.features.length === 0}
			<div class="empty-state">No features yet</div>
		{:else}
			{#each tree.features as feature, i (feature.id)}
				{@const isAfterRollback = tree.active_index !== null && i > tree.active_index}
				{@const isDragging = dragFeatureId === feature.id}
				<div
					class="tree-item"
					class:selected={selectedId === feature.id}
					class:suppressed={feature.suppressed}
					class:after-rollback={isAfterRollback}
					class:dragging={isDragging}
					class:drop-above={dropTargetIndex === i && dragFeatureId !== feature.id}
					draggable="true"
					onclick={() => handleClick(feature.id)}
					ondblclick={() => handleDblClick(feature)}
					oncontextmenu={(e) => handleContextMenu(e, feature)}
					ondragstart={(e) => handleDragStart(e, feature)}
					ondragover={(e) => handleDragOver(e, i)}
					ondragleave={handleDragLeave}
					ondrop={(e) => handleDrop(e, i)}
					ondragend={handleDragEnd}
					role="treeitem"
					tabindex="0"
				>
					<span class="tree-icon">{featureIcon(feature.operation?.type)}</span>
					{#if renaming && renaming.featureId === feature.id}
						<input
							class="rename-input"
							bind:value={renaming.value}
							onkeydown={handleRename}
							onblur={handleRenameBlur}
						/>
					{:else}
						<span class="tree-label">{feature.name}</span>
					{/if}
					{#if feature.suppressed}
						<span class="suppress-indicator" title="Suppressed">S</span>
					{/if}
				</div>
			{/each}
		{/if}
	</div>

	{#if tree.features.length > 0}
		<div class="rollback-area">
			<label class="rollback-label">
				Rollback
				<input
					type="range"
					class="rollback-slider"
					min="0"
					max={tree.features.length}
					value={rollbackValue}
					oninput={handleRollback}
				/>
			</label>
		</div>
	{/if}
</div>

<!-- Context Menu -->
{#if contextMenu}
	<div
		class="context-menu"
		style="left: {contextMenu.x}px; top: {contextMenu.y}px"
		onclick={(e) => e.stopPropagation()}
	>
		<button class="ctx-item" onclick={handleSuppress}>
			{contextMenu.suppressed ? 'Unsuppress' : 'Suppress'}
		</button>
		<button class="ctx-item danger" onclick={handleDelete}>Delete</button>
	</div>
{/if}

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
		overflow-y: auto;
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
		cursor: grab;
		gap: 6px;
		user-select: none;
		transition: border-top 0.1s;
		border-top: 2px solid transparent;
	}

	.tree-item:hover {
		background: var(--bg-hover);
	}

	.tree-item.selected {
		background: rgba(0, 120, 212, 0.2);
		border-left: 2px solid var(--accent);
		padding-left: 10px;
	}

	.tree-item.suppressed {
		opacity: 0.4;
		text-decoration: line-through;
	}

	.tree-item.after-rollback {
		opacity: 0.3;
	}

	.tree-item.dragging {
		opacity: 0.4;
	}

	.tree-item.drop-above {
		border-top: 2px solid var(--accent);
	}

	.tree-icon {
		width: 16px;
		text-align: center;
		font-size: 12px;
		color: var(--text-secondary);
		flex-shrink: 0;
	}

	.tree-label {
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.suppress-indicator {
		margin-left: auto;
		font-size: 9px;
		color: var(--text-muted);
		background: var(--bg-tertiary);
		padding: 0 3px;
		border-radius: 2px;
	}

	.rename-input {
		background: var(--bg-primary);
		border: 1px solid var(--accent);
		color: var(--text-primary);
		font-size: 12px;
		padding: 1px 4px;
		outline: none;
		flex: 1;
		min-width: 0;
	}

	.rollback-area {
		padding: 6px 12px;
		border-top: 1px solid var(--border-color);
		background: var(--bg-tertiary);
	}

	.rollback-label {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 10px;
		color: var(--text-secondary);
	}

	.rollback-slider {
		flex: 1;
		height: 4px;
		accent-color: var(--accent);
	}

	.context-menu {
		position: fixed;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 4px;
		padding: 4px 0;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
		min-width: 120px;
	}

	.ctx-item {
		display: block;
		width: 100%;
		background: transparent;
		border: none;
		color: var(--text-primary);
		font-size: 12px;
		padding: 5px 16px;
		cursor: pointer;
		text-align: left;
	}

	.ctx-item:hover {
		background: var(--accent);
		color: white;
	}

	.ctx-item.danger:hover {
		background: var(--error);
	}
</style>
