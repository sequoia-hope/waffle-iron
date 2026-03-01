<script>
	import {
		getTestCaseBrowserState,
		hideTestCaseBrowser,
		refreshTestCases,
		showSaveTestCaseDialog,
		loadTestCase,
		removeTestCase
	} from '$lib/engine/store.svelte.js';

	let state = $derived(getTestCaseBrowserState());
	let filterOutcome = $state('all');
	let filterTag = $state('');

	let filtered = $derived.by(() => {
		let cases = state.cases;
		if (filterOutcome !== 'all') {
			cases = cases.filter(c => c.expectedOutcome === filterOutcome);
		}
		if (filterTag.trim()) {
			const tag = filterTag.trim().toLowerCase();
			cases = cases.filter(c => c.tags?.some(t => t.toLowerCase().includes(tag)));
		}
		return cases;
	});

	function outcomeDot(outcome) {
		switch (outcome) {
			case 'should_pass': return 'dot-pass';
			case 'known_failure': return 'dot-warn';
			case 'regression': return 'dot-fail';
			default: return 'dot-pass';
		}
	}

	function outcomeLabel(outcome) {
		switch (outcome) {
			case 'should_pass': return 'Pass';
			case 'known_failure': return 'Known Fail';
			case 'regression': return 'Regression';
			default: return outcome;
		}
	}
</script>

{#if state.visible}
<div class="test-case-browser" data-testid="test-case-browser">
	<div class="tcb-header">
		<span class="tcb-title">Test Cases</span>
		<div class="tcb-header-actions">
			<button class="tcb-icon-btn" title="Refresh" data-testid="tcb-refresh" onclick={() => refreshTestCases()}>&#x21bb;</button>
			<button class="tcb-icon-btn" title="Close" data-testid="tcb-close" onclick={() => hideTestCaseBrowser()}>&#x2715;</button>
		</div>
	</div>

	<div class="tcb-toolbar">
		<button class="tcb-save-btn" data-testid="tcb-save-current" onclick={() => showSaveTestCaseDialog()}>+ Save Current</button>
	</div>

	<div class="tcb-filters">
		<select class="tcb-filter-select" data-testid="tcb-filter-outcome" bind:value={filterOutcome}>
			<option value="all">All outcomes</option>
			<option value="should_pass">Should pass</option>
			<option value="known_failure">Known failure</option>
			<option value="regression">Regression</option>
		</select>
		<input
			class="tcb-filter-input"
			type="text"
			placeholder="Filter by tag..."
			data-testid="tcb-filter-tag"
			bind:value={filterTag}
		/>
	</div>

	<div class="tcb-list" data-testid="tcb-list">
		{#if state.loading}
			<div class="tcb-empty">Loading...</div>
		{:else if state.error}
			<div class="tcb-empty tcb-error">{state.error}</div>
		{:else if filtered.length === 0}
			<div class="tcb-empty">No test cases{filterOutcome !== 'all' || filterTag ? ' matching filter' : ''}. Click "+ Save Current" to create one.</div>
		{:else}
			{#each filtered as tc (tc.id)}
				<div class="tcb-case" data-testid="tcb-case-{tc.id}">
					<div class="tcb-case-header">
						<span class="tcb-dot {outcomeDot(tc.expectedOutcome)}" title={outcomeLabel(tc.expectedOutcome)}></span>
						<span class="tcb-case-name">{tc.name}</span>
					</div>
					{#if tc.description}
						<div class="tcb-case-desc">{tc.description}</div>
					{/if}
					{#if tc.tags?.length > 0}
						<div class="tcb-case-tags">
							{#each tc.tags as tag}
								<span class="tcb-tag">{tag}</span>
							{/each}
						</div>
					{/if}
					<div class="tcb-case-actions">
						<button class="tcb-action-btn" data-testid="tcb-load-{tc.id}" onclick={() => loadTestCase(tc.id)}>Load</button>
						<button class="tcb-action-btn tcb-delete-btn" data-testid="tcb-delete-{tc.id}" onclick={() => removeTestCase(tc.id)}>Delete</button>
					</div>
				</div>
			{/each}
		{/if}
	</div>
</div>
{/if}

<style>
	.test-case-browser {
		position: absolute;
		top: 0;
		right: 0;
		width: 280px;
		height: 100%;
		background: var(--bg-secondary);
		border-left: 1px solid var(--border-color);
		z-index: 50;
		display: flex;
		flex-direction: column;
		font-size: 12px;
	}

	.tcb-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 10px;
		border-bottom: 1px solid var(--border-color);
	}

	.tcb-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary);
	}

	.tcb-header-actions {
		display: flex;
		gap: 4px;
	}

	.tcb-icon-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 14px;
		padding: 2px 4px;
		border-radius: 3px;
	}

	.tcb-icon-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.tcb-toolbar {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border-color);
	}

	.tcb-save-btn {
		width: 100%;
		padding: 5px 8px;
		background: var(--accent);
		color: white;
		border: none;
		border-radius: 3px;
		cursor: pointer;
		font-size: 12px;
		font-weight: 500;
	}

	.tcb-save-btn:hover {
		opacity: 0.9;
	}

	.tcb-filters {
		padding: 6px 10px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		border-bottom: 1px solid var(--border-color);
	}

	.tcb-filter-select,
	.tcb-filter-input {
		width: 100%;
		padding: 4px 6px;
		font-size: 11px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		outline: none;
	}

	.tcb-filter-select:focus,
	.tcb-filter-input:focus {
		border-color: var(--accent);
	}

	.tcb-list {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.tcb-empty {
		padding: 20px 10px;
		text-align: center;
		color: var(--text-muted);
	}

	.tcb-error {
		color: var(--error);
	}

	.tcb-case {
		padding: 8px 10px;
		border-bottom: 1px solid var(--border-color);
	}

	.tcb-case:hover {
		background: var(--bg-hover);
	}

	.tcb-case-header {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 2px;
	}

	.tcb-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.dot-pass { background: var(--success, #4ec9b0); }
	.dot-warn { background: var(--warning, #cca700); }
	.dot-fail { background: var(--error, #f44747); }

	.tcb-case-name {
		font-weight: 500;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.tcb-case-desc {
		color: var(--text-secondary);
		font-size: 11px;
		margin-bottom: 4px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.tcb-case-tags {
		display: flex;
		flex-wrap: wrap;
		gap: 3px;
		margin-bottom: 4px;
	}

	.tcb-tag {
		background: var(--bg-primary);
		color: var(--text-secondary);
		padding: 1px 5px;
		border-radius: 2px;
		font-size: 10px;
	}

	.tcb-case-actions {
		display: flex;
		gap: 4px;
	}

	.tcb-action-btn {
		padding: 3px 8px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		cursor: pointer;
		font-size: 11px;
	}

	.tcb-action-btn:hover {
		background: var(--bg-hover);
		border-color: var(--accent);
	}

	.tcb-delete-btn:hover {
		border-color: var(--error, #f44747);
		color: var(--error, #f44747);
	}

	@media (max-width: 768px) {
		.test-case-browser {
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
