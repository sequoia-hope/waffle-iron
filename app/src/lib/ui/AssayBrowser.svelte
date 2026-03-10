<script>
	import {
		getAssayBrowserState,
		hideAssayBrowser,
		refreshAssayCases,
		loadAssayCase
	} from '$lib/engine/store.svelte.js';

	let state = $derived(getAssayBrowserState());

	let searchTerm = $state('');

	let filtered = $derived.by(() => {
		let cases = state.cases;
		if (searchTerm.trim()) {
			const term = searchTerm.trim().toLowerCase();
			cases = cases.filter(c =>
				c.id.toLowerCase().includes(term) ||
				(c.description && c.description.toLowerCase().includes(term))
			);
		}
		return cases;
	});

	let activeMeta = $derived(state.activeMeta);
	let activeCase = $derived(state.activeCase);
</script>

{#if state.visible}
<div class="assay-browser" data-testid="assay-browser">
	<div class="ab-header">
		<span class="ab-title">Assay Browser</span>
		<span class="ab-count">{state.cases.length} cases</span>
		<div class="ab-header-actions">
			<button class="ab-icon-btn" title="Refresh" data-testid="assay-refresh" onclick={() => refreshAssayCases()}>&#x21bb;</button>
			<button class="ab-icon-btn" title="Close" data-testid="assay-browser-close" onclick={() => hideAssayBrowser()}>&#x2715;</button>
		</div>
	</div>

	<div class="ab-filters">
		<input
			class="ab-search"
			type="text"
			placeholder="Search cases..."
			data-testid="assay-search"
			bind:value={searchTerm}
		/>
	</div>

	<div class="ab-body">
		<div class="ab-list" data-testid="assay-case-list">
			{#if state.loading}
				<div class="ab-empty">Loading...</div>
			{:else if state.error}
				<div class="ab-empty ab-error">{state.error}</div>
			{:else if filtered.length === 0}
				<div class="ab-empty">No assay cases{searchTerm ? ' matching filter' : ' generated yet'}.</div>
			{:else}
				{#each filtered as c (c.id)}
					<button
						class="ab-case-item"
						class:active={activeCase === c.id}
						data-testid="assay-case-{c.id}"
						onclick={() => loadAssayCase(c.id)}
					>
						<span class="ab-case-id">{c.id}</span>
						<span class="ab-case-desc">{c.description}</span>
					</button>
				{/each}
			{/if}
		</div>

		{#if activeMeta}
			<div class="ab-oracle-overlay" data-testid="assay-oracle-overlay">
				<h3 class="ab-oracle-title">Oracle Expectations</h3>
				{#if activeMeta.oracles}
					<div class="ab-oracle-row">Euler: {activeMeta.oracles.euler_target}</div>
					<div class="ab-oracle-row">Watertight: {activeMeta.oracles.expect_watertight ? 'Yes' : 'No'}</div>
					<div class="ab-oracle-row">Volume: {activeMeta.oracles.expect_positive_volume ? 'Positive' : 'Any'}</div>
					<div class="ab-oracle-row">BBox max: {activeMeta.oracles.max_bbox_extent?.toFixed(2) ?? '?'}m</div>
				{/if}
				{#if activeMeta.scale != null}
					<div class="ab-oracle-row">Scale: {activeMeta.scale.toExponential(2)}</div>
				{/if}
				{#if activeMeta.operations}
					<div class="ab-oracle-row">Ops: {activeMeta.operations.map(o => `${o.kind}(${o.profile_type})`).join(', ')}</div>
				{/if}
			</div>
		{/if}
	</div>
</div>
{/if}

<style>
	.assay-browser {
		position: absolute;
		top: 0;
		right: 0;
		width: 300px;
		height: 100%;
		background: var(--bg-secondary);
		border-left: 1px solid var(--border-color);
		z-index: 50;
		display: flex;
		flex-direction: column;
		font-size: 12px;
	}

	.ab-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border-bottom: 1px solid var(--border-color);
	}

	.ab-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary);
	}

	.ab-count {
		font-size: 11px;
		color: var(--text-secondary);
		background: var(--bg-primary);
		padding: 1px 6px;
		border-radius: 8px;
	}

	.ab-header-actions {
		margin-left: auto;
		display: flex;
		gap: 4px;
	}

	.ab-icon-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 14px;
		padding: 2px 4px;
		border-radius: 3px;
	}

	.ab-icon-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.ab-filters {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border-color);
	}

	.ab-search {
		width: 100%;
		padding: 4px 6px;
		font-size: 11px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		outline: none;
	}

	.ab-search:focus {
		border-color: var(--accent);
	}

	.ab-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.ab-list {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.ab-empty {
		padding: 20px 10px;
		text-align: center;
		color: var(--text-muted);
	}

	.ab-error {
		color: var(--error);
	}

	.ab-case-item {
		display: flex;
		flex-direction: column;
		width: 100%;
		padding: 6px 10px;
		border: none;
		border-bottom: 1px solid var(--border-color);
		background: none;
		cursor: pointer;
		text-align: left;
		color: var(--text-primary);
		font-size: 12px;
	}

	.ab-case-item:hover {
		background: var(--bg-hover);
	}

	.ab-case-item.active {
		background: rgba(0, 120, 212, 0.15);
		border-left: 2px solid var(--accent);
	}

	.ab-case-id {
		font-weight: 600;
		font-size: 11px;
		color: var(--accent);
		font-family: monospace;
	}

	.ab-case-desc {
		font-size: 11px;
		color: var(--text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ab-oracle-overlay {
		padding: 8px 10px;
		border-top: 1px solid var(--border-color);
		background: var(--bg-tertiary);
		font-size: 11px;
	}

	.ab-oracle-title {
		font-size: 12px;
		font-weight: 600;
		margin: 0 0 4px;
		color: var(--text-primary);
	}

	.ab-oracle-row {
		color: var(--text-secondary);
		padding: 1px 0;
	}

	@media (max-width: 768px) {
		.assay-browser {
			width: 100%;
			height: 60vh;
			top: auto;
			bottom: 0;
			left: 0;
			right: 0;
			position: fixed;
			border-radius: 12px 12px 0 0;
			border-left: none;
			border-top: 1px solid var(--border-color);
			z-index: 150;
		}
	}
</style>
