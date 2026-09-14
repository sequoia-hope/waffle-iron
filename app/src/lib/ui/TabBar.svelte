<script>
	let {
		tabs = [],
		activeTabId = null,
		onswitch,
		onclose,
		onadd,
		onrename,
		onmove
	} = $props();

	let editingId = $state(null);
	let editValue = $state('');

	// Drag-to-reorder: the tab being dragged and the gap it would drop into
	// (0 = before the first tab, tabs.length = after the last).
	let dragId = $state(null);
	let dropGap = $state(null);

	function handleDoubleClick(tab) {
		editingId = tab.id;
		editValue = tab.name;
	}

	function commitRename() {
		if (editingId && editValue.trim() && onrename) {
			onrename(editingId, editValue.trim());
		}
		editingId = null;
	}

	function handleKeydown(e) {
		if (e.key === 'Enter') commitRename();
		else if (e.key === 'Escape') editingId = null;
	}

	function handleDragStart(e, tab) {
		if (editingId || !onmove) {
			e.preventDefault();
			return;
		}
		dragId = tab.id;
		e.dataTransfer.effectAllowed = 'move';
		e.dataTransfer.setData('text/plain', tab.id);
	}

	function handleDragOver(e, index) {
		if (dragId === null) return;
		e.preventDefault();
		e.dataTransfer.dropEffect = 'move';
		const rect = e.currentTarget.getBoundingClientRect();
		dropGap = e.clientX < rect.left + rect.width / 2 ? index : index + 1;
	}

	function handleDrop(e) {
		e.preventDefault();
		const from = tabs.findIndex((t) => t.id === dragId);
		if (from !== -1 && dropGap !== null) {
			// Removing the tab first shifts every gap after it left by one.
			const to = dropGap > from ? dropGap - 1 : dropGap;
			if (to !== from) onmove?.(dragId, to);
		}
		endDrag();
	}

	function endDrag() {
		dragId = null;
		dropGap = null;
	}
</script>

<div class="tab-bar" data-testid="tab-bar">
	<div class="tab-list">
		{#each tabs as tab, i (tab.id)}
			<button
				class="tab"
				class:active={tab.id === activeTabId}
				class:dragging={tab.id === dragId}
				class:drop-before={dragId !== null && dropGap === i}
				class:drop-after={dragId !== null && dropGap === i + 1 && i === tabs.length - 1}
				data-testid="tab-{tab.id}"
				draggable={editingId !== tab.id}
				onclick={() => onswitch?.(tab.id)}
				ondblclick={() => handleDoubleClick(tab)}
				ondragstart={(e) => handleDragStart(e, tab)}
				ondragover={(e) => handleDragOver(e, i)}
				ondrop={handleDrop}
				ondragend={endDrag}
			>
				{#if editingId === tab.id}
					<input
						class="tab-rename-input"
						bind:value={editValue}
						onblur={commitRename}
						onkeydown={handleKeydown}
					/>
				{:else}
					<span class="tab-name">{tab.name}</span>
				{/if}
				{#if tabs.length > 1}
					<button
						class="tab-close"
						data-testid="tab-close-{tab.id}"
						onclick={(e) => { e.stopPropagation(); onclose?.(tab.id); }}
						title="Close tab"
					>&times;</button>
				{/if}
			</button>
		{/each}
	</div>
	<button class="tab-add" data-testid="tab-add" onclick={() => onadd?.('Part')} title="New part tab">+</button>
	<button class="tab-add tab-add-assembly" data-testid="tab-add-assembly" onclick={() => onadd?.('Assembly')} title="New assembly tab">+ Asm</button>
</div>

<style>
	.tab-bar {
		display: flex;
		align-items: center;
		background: var(--bg-secondary, #313244);
		border-bottom: 1px solid var(--border-color, #45475a);
		height: 32px;
		padding: 0 4px;
		gap: 2px;
		overflow-x: auto;
		scrollbar-width: none;
	}

	.tab-bar::-webkit-scrollbar {
		display: none;
	}

	.tab-list {
		display: flex;
		gap: 2px;
		min-width: 0;
	}

	.tab {
		display: flex;
		align-items: center;
		gap: 4px;
		background: transparent;
		border: none;
		color: var(--text-secondary, #a6adc8);
		padding: 4px 12px;
		border-radius: 4px 4px 0 0;
		cursor: pointer;
		font-size: 12px;
		white-space: nowrap;
		min-width: 0;
		transition: background 0.1s, color 0.1s;
	}

	.tab:hover {
		background: var(--bg-hover, rgba(255,255,255,0.05));
	}

	.tab.active {
		background: var(--bg-primary, #1e1e2e);
		color: var(--text-primary, #cdd6f4);
		border-bottom: 2px solid var(--accent, #0078d4);
	}

	.tab.dragging {
		opacity: 0.5;
	}

	.tab.drop-before {
		box-shadow: inset 2px 0 0 var(--accent, #0078d4);
	}

	.tab.drop-after {
		box-shadow: inset -2px 0 0 var(--accent, #0078d4);
	}

	.tab-name {
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 120px;
	}

	.tab-close {
		background: none;
		border: none;
		color: var(--text-muted, #6c7086);
		cursor: pointer;
		padding: 0 2px;
		font-size: 14px;
		line-height: 1;
		border-radius: 2px;
	}

	.tab-close:hover {
		color: var(--text-primary, #cdd6f4);
		background: rgba(255, 255, 255, 0.1);
	}

	.tab-rename-input {
		background: var(--bg-primary, #1e1e2e);
		border: 1px solid var(--accent, #0078d4);
		color: var(--text-primary, #cdd6f4);
		font-size: 12px;
		padding: 1px 4px;
		border-radius: 2px;
		width: 80px;
		outline: none;
	}

	.tab-add {
		background: none;
		border: none;
		color: var(--text-muted, #6c7086);
		cursor: pointer;
		font-size: 16px;
		padding: 2px 8px;
		border-radius: 4px;
		flex-shrink: 0;
	}

	.tab-add:hover {
		color: var(--text-primary, #cdd6f4);
		background: var(--bg-hover, rgba(255,255,255,0.05));
	}
</style>
