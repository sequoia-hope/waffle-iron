<script>
	import {
		getAssayBrowserState,
		hideAssayBrowser,
		refreshAssayCases,
		loadAssayCase
	} from '$lib/engine/store.svelte.js';
	import { bottomSheetResize } from './bottomSheetResize.js';

	let state = $derived(getAssayBrowserState());

	let searchTerm = $state('');

	// Status sort order: pass first, then fail, then error, then unknown
	const statusOrder = { pass: 0, fail: 1, error: 2 };

	let filtered = $derived.by(() => {
		let cases = state.cases;
		if (searchTerm.trim()) {
			const term = searchTerm.trim().toLowerCase();
			const results = state.results || {};
			cases = cases.filter(c => {
				// Search across all visible attributes
				if (c.id.toLowerCase().includes(term)) return true;
				if (c.description && c.description.toLowerCase().includes(term)) return true;
				if (c.featured && 'featured'.includes(term)) return true;
				const r = results[c.id];
				if (r) {
					if (r.status && r.status.toLowerCase().includes(term)) return true;
					if (r.category && r.category.toLowerCase().includes(term)) return true;
					if (r.detail && r.detail.toLowerCase().includes(term)) return true;
				}
				return false;
			});
		}
		// Sort: featured first, then by status (pass, fail, error), then by ID
		const results = state.results || {};
		return [...cases].sort((a, b) => {
			const fa = a.featured ? 0 : 1;
			const fb = b.featured ? 0 : 1;
			if (fa !== fb) return fa - fb;
			const sa = statusOrder[results[a.id]?.status] ?? 3;
			const sb = statusOrder[results[b.id]?.status] ?? 3;
			if (sa !== sb) return sa - sb;
			return a.id.localeCompare(b.id);
		});
	});

	let activeMeta = $derived(state.activeMeta);
	let activeCase = $derived(state.activeCase);

	function statusLabel(id) {
		const r = state.results?.[id];
		if (!r) return null;
		return r.status;
	}

	// PR-ASSAY-VOID: a passing case whose pass MEANS "the engine errored as
	// the meta expects" (revolve self-intersection canaries etc.). The error
	// toast the case load produces is the EXPECTED outcome — badge it so
	// PASS + error toast reads as intended, not contradictory.
	function isExpectedError(id) {
		const r = state.results?.[id];
		return !!(r?.status === 'pass' && r.detail && r.detail.startsWith('expected rebuild error'));
	}

	// Count results
	let passCount = $derived(Object.values(state.results || {}).filter(r => r.status === 'pass').length);
	let totalResults = $derived(Object.keys(state.results || {}).length);
	let featuredCount = $derived(state.cases.filter(c => c.featured).length);
</script>

{#if state.visible}
<div class="assay-browser" data-testid="assay-browser">
	<div class="ab-header" use:bottomSheetResize>
		<span class="ab-title">Assay Browser</span>
		<span class="ab-count">{state.cases.length} cases</span>
		{#if totalResults > 0}
			<span class="ab-score" data-testid="assay-score">{passCount}/{totalResults}</span>
		{/if}
		{#if featuredCount > 0}
			<span class="ab-featured-count">{featuredCount} featured</span>
		{/if}
		<div class="ab-header-actions">
			<button class="ab-icon-btn" title="Refresh" data-testid="assay-refresh" onclick={() => refreshAssayCases()}>&#x21bb;</button>
			<button class="ab-icon-btn" title="Close" data-testid="assay-browser-close" onclick={() => hideAssayBrowser()}>&#x2715;</button>
		</div>
	</div>

	<div class="ab-filters">
		<input
			class="ab-search"
			type="text"
			placeholder="Search id, ops, status, category..."
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
					{@const status = statusLabel(c.id)}
					<button
						class="ab-case-item"
						class:active={activeCase === c.id}
						class:status-pass={status === 'pass'}
						class:status-fail={status === 'fail'}
						class:status-error={status === 'error'}
						class:status-featured={c.featured}
						data-testid="assay-case-{c.id}"
						onclick={() => loadAssayCase(c.id)}
					>
						<div class="ab-case-row">
							{#if c.featured}
								<span class="ab-status-badge featured">FEATURED</span>
							{/if}
							{#if status}
								<span class="ab-status-badge" class:pass={status === 'pass'} class:fail={status === 'fail'} class:error={status === 'error'}>
									{status === 'pass' ? (isExpectedError(c.id) ? 'PASS (EXPECTED ERR)' : 'PASS') : status === 'fail' ? 'FAIL' : 'ERR'}
								</span>
							{/if}
							<span class="ab-case-id">{c.id}</span>
						</div>
						<span class="ab-case-desc">{c.description}</span>
						{#if state.results?.[c.id]?.category}
							<span class="ab-case-category">{state.results[c.id].category}</span>
						{/if}
					</button>
				{/each}
			{/if}
		</div>

		{#if activeMeta}
			<div class="ab-oracle-overlay" data-testid="assay-oracle-overlay">
				<h3 class="ab-oracle-title">Oracle Expectations</h3>
				{#if activeMeta.oracles}
					{#if activeMeta.oracles.expect_rebuild_error}
						<div class="ab-oracle-row ab-expected-error">
							Expects rebuild error: YES — the engine error this case raises IS the
							expected outcome (PASS means the error fired)
						</div>
					{/if}
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
				{#if activeCase && state.results?.[activeCase]}
					<div class="ab-oracle-row ab-result-detail">Result: {state.results[activeCase].detail}</div>
				{/if}
			</div>
		{/if}
	</div>
</div>
{/if}

<style>
	.ab-expected-error {
		color: #e0b34d;
		font-weight: 600;
	}

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

	.ab-score {
		font-size: 11px;
		font-weight: 600;
		color: #4caf50;
		background: rgba(76, 175, 80, 0.12);
		padding: 1px 6px;
		border-radius: 8px;
		font-family: monospace;
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

	.ab-case-item.status-pass {
		border-left: 2px solid #4caf50;
	}

	.ab-case-item.status-fail {
		border-left: 2px solid #f44336;
	}

	.ab-case-item.status-error {
		border-left: 2px solid #ff9800;
	}

	.ab-case-item.active.status-pass {
		background: rgba(76, 175, 80, 0.1);
	}

	.ab-case-row {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.ab-status-badge {
		display: inline-block;
		font-size: 9px;
		font-weight: 700;
		padding: 1px 4px;
		border-radius: 3px;
		font-family: monospace;
		letter-spacing: 0.5px;
		line-height: 1.4;
		flex-shrink: 0;
	}

	.ab-status-badge.pass {
		background: rgba(76, 175, 80, 0.2);
		color: #4caf50;
	}

	.ab-status-badge.fail {
		background: rgba(244, 67, 54, 0.15);
		color: #f44336;
	}

	.ab-status-badge.error {
		background: rgba(255, 152, 0, 0.15);
		color: #ff9800;
	}

	.ab-status-badge.featured {
		background: rgba(255, 193, 7, 0.2);
		color: #ffc107;
	}

	.ab-case-item.status-featured {
		border-left: 2px solid #ffc107;
	}

	.ab-featured-count {
		font-size: 11px;
		color: #ffc107;
		background: rgba(255, 193, 7, 0.12);
		padding: 1px 6px;
		border-radius: 8px;
		font-family: monospace;
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

	.ab-case-category {
		font-size: 10px;
		color: var(--text-muted);
		font-style: italic;
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

	.ab-result-detail {
		margin-top: 4px;
		font-size: 10px;
		word-break: break-word;
		max-height: 60px;
		overflow-y: auto;
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

		.ab-header {
			flex-wrap: wrap;
			justify-content: center;
			cursor: grab;
		}

		.ab-header::before {
			content: '';
			display: block;
			width: 32px;
			height: 4px;
			background: var(--text-muted);
			opacity: 0.4;
			border-radius: 2px;
			flex-basis: 100%;
			margin: 2px auto 4px;
		}
	}
</style>
