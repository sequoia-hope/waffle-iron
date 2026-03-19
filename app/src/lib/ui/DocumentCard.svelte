<script>
	import { spring } from 'svelte/motion';
	import { timeago } from '$lib/utils/timeago.js';
	import ThumbnailViewport from './ThumbnailViewport.svelte';

	let { doc, index = 0, onclick, onrename, ondelete } = $props();

	const scale = spring(0.96, { stiffness: 0.12, damping: 0.7 });

	let showMenu = $state(false);
	let menuX = $state(0);
	let menuY = $state(0);
	let editing = $state(false);
	let editValue = $state('');

	// Staggered appear animation
	$effect(() => {
		const delay = index * 40;
		const timeout = setTimeout(() => scale.set(1.0), delay);
		return () => clearTimeout(timeout);
	});

	function handleMouseEnter() {
		scale.set(1.02);
	}

	function handleMouseLeave() {
		scale.set(1.0);
	}

	function handleClick() {
		if (editing) return;
		scale.set(0.98);
		setTimeout(() => {
			if (onclick) onclick(doc);
		}, 120);
	}

	function handleContextMenu(e) {
		e.preventDefault();
		menuX = e.clientX;
		menuY = e.clientY;
		showMenu = true;
	}

	function closeMenu() {
		showMenu = false;
	}

	function startRename() {
		showMenu = false;
		editing = true;
		editValue = doc.name;
	}

	function commitRename() {
		if (editValue.trim() && editValue.trim() !== doc.name) {
			onrename?.(doc, editValue.trim());
		}
		editing = false;
	}

	function handleRenameKeydown(e) {
		if (e.key === 'Enter') commitRename();
		else if (e.key === 'Escape') editing = false;
	}

	function handleDelete() {
		showMenu = false;
		ondelete?.(doc);
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if showMenu}
	<div class="ctx-backdrop" onclick={closeMenu}></div>
	<div class="ctx-menu" style="left:{menuX}px;top:{menuY}px" data-testid="doc-context-menu">
		<button class="ctx-item" data-testid="doc-ctx-rename" onclick={startRename}>Rename</button>
		<button class="ctx-item ctx-delete" data-testid="doc-ctx-delete" onclick={handleDelete}>Delete</button>
	</div>
{/if}

<button
	class="document-card"
	data-testid="document-card"
	data-doc-id={doc.id}
	style="transform: scale({$scale})"
	onmouseenter={handleMouseEnter}
	onmouseleave={handleMouseLeave}
	onclick={handleClick}
	oncontextmenu={handleContextMenu}
>
	<div class="card-thumb">
		{#if doc.previewMesh}
			<ThumbnailViewport previewMesh={doc.previewMesh} width={240} height={180} />
		{:else}
			<div class="thumb-placeholder">
				<svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
					<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
					<polyline points="3.27 6.96 12 12.01 20.73 6.96"/>
					<line x1="12" y1="22.08" x2="12" y2="12"/>
				</svg>
			</div>
		{/if}
	</div>
	<div class="card-info">
		{#if editing}
			<!-- svelte-ignore a11y_autofocus -->
			<input
				class="card-rename-input"
				data-testid="doc-rename-input"
				bind:value={editValue}
				onblur={commitRename}
				onkeydown={handleRenameKeydown}
				onclick={(e) => e.stopPropagation()}
				autofocus
			/>
		{:else}
			<span class="card-name">{doc.name}</span>
		{/if}
		<span class="card-meta">
			{doc.tabCount > 1 ? `${doc.tabCount} tabs` : ''}
			{doc.tabCount > 1 ? ' · ' : ''}{timeago(doc.modified)}
		</span>
	</div>
</button>

<style>
	.document-card {
		background: var(--bg-secondary, #313244);
		border: 1px solid var(--border-color, #45475a);
		border-radius: 8px;
		overflow: hidden;
		cursor: pointer;
		text-align: left;
		padding: 0;
		color: inherit;
		transition: box-shadow 0.2s;
		transform-origin: center;
	}

	.document-card:hover {
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
	}

	.card-thumb {
		aspect-ratio: 4/3;
		background: var(--bg-primary, #1e1e2e);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-muted, #6c7086);
	}

	.thumb-placeholder {
		opacity: 0.4;
	}

	.card-info {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.card-name {
		font-size: 14px;
		font-weight: 500;
		color: var(--text-primary, #cdd6f4);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.card-meta {
		font-size: 12px;
		color: var(--text-secondary, #a6adc8);
	}

	.card-rename-input {
		font-size: 14px;
		font-weight: 500;
		color: var(--text-primary, #cdd6f4);
		background: var(--bg-primary, #1e1e2e);
		border: 1px solid var(--accent, #0078d4);
		border-radius: 3px;
		padding: 2px 4px;
		width: 100%;
		outline: none;
	}

	.ctx-backdrop {
		position: fixed;
		inset: 0;
		z-index: 999;
	}

	.ctx-menu {
		position: fixed;
		z-index: 1000;
		background: var(--bg-secondary, #313244);
		border: 1px solid var(--border-color, #45475a);
		border-radius: 6px;
		padding: 4px 0;
		min-width: 140px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
	}

	.ctx-item {
		display: block;
		width: 100%;
		padding: 8px 16px;
		background: none;
		border: none;
		color: var(--text-primary, #cdd6f4);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
	}

	.ctx-item:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.05));
	}

	.ctx-delete {
		color: #f38ba8;
	}

	.ctx-delete:hover {
		background: rgba(243, 139, 168, 0.1);
	}
</style>
