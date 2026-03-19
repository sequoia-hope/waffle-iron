<script>
	import { spring } from 'svelte/motion';
	import { timeago } from '$lib/utils/timeago.js';
	import ThumbnailViewport from './ThumbnailViewport.svelte';

	let { doc, index = 0, onclick } = $props();

	const scale = spring(0.96, { stiffness: 0.12, damping: 0.7 });

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
		scale.set(0.98);
		setTimeout(() => {
			if (onclick) onclick(doc);
		}, 120);
	}
</script>

<button
	class="document-card"
	data-testid="document-card"
	style="transform: scale({$scale})"
	onmouseenter={handleMouseEnter}
	onmouseleave={handleMouseLeave}
	onclick={handleClick}
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
		<span class="card-name">{doc.name}</span>
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
</style>
