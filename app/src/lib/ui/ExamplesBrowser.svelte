<script>
	/**
	 * The Examples panel: the official example documents shipped in
	 * `app/static/examples/` (toolbar → Examples, beside Assay). Opening one
	 * loads a copy as a new document; the details show what the example
	 * exercises and link its generator when it has one. On the development
	 * server the open document can be saved as a new example.
	 */
	import {
		getExamplesBrowserState,
		hideExamplesBrowser,
		refreshExamples,
		loadExample,
		saveAsExample,
		getProjectName
	} from '$lib/engine/store.svelte.js';
	import { exampleGeneratorUrl } from '$lib/engine/examplesApi.js';
	import { bottomSheetResize } from './bottomSheetResize.js';

	let state = $derived(getExamplesBrowserState());
	let active = $derived(state.examples.find((e) => e.id === state.active) ?? null);

	let showSave = $state(false);
	let saveName = $state('');
	let saveDescription = $state('');

	function openSave() {
		saveName = getProjectName() || 'Untitled';
		saveDescription = '';
		showSave = true;
	}

	async function submitSave() {
		if (!saveName.trim()) return;
		if (await saveAsExample(saveName.trim(), saveDescription.trim())) showSave = false;
	}
</script>

{#if state.visible}
<div class="examples-browser" data-testid="examples-browser">
	<div class="eb-header" use:bottomSheetResize>
		<span class="eb-title">Examples</span>
		<span class="eb-count" data-testid="examples-count">{state.examples.length}</span>
		<div class="eb-header-actions">
			<button class="eb-icon-btn" title="Refresh" data-testid="examples-refresh" onclick={() => refreshExamples()}>&#x21bb;</button>
			<button class="eb-icon-btn" title="Close" data-testid="examples-browser-close" onclick={() => hideExamplesBrowser()}>&#x2715;</button>
		</div>
	</div>

	<div class="eb-body">
		<div class="eb-list" data-testid="examples-list">
			{#if state.loading}
				<div class="eb-empty">Loading...</div>
			{:else if state.error}
				<div class="eb-empty eb-error">{state.error}</div>
			{:else if state.examples.length === 0}
				<div class="eb-empty">No examples shipped yet.</div>
			{:else}
				{#each state.examples as ex (ex.id)}
					<button
						class="eb-item"
						class:active={state.active === ex.id}
						disabled={state.opening !== null}
						data-testid="example-{ex.id}"
						title="Open a copy of this example as a new document"
						onclick={() => loadExample(ex.id)}
					>
						<div class="eb-item-row">
							<span class="eb-item-name">{ex.name}</span>
							{#if state.opening === ex.id}
								<span class="eb-badge">OPENING…</span>
							{:else if ex.generator}
								<span class="eb-badge">GENERATED</span>
							{/if}
						</div>
						<span class="eb-item-desc">{ex.description}</span>
						{#if ex.tabs?.length}
							<span class="eb-item-meta">{ex.tabs.length} tabs{ex.built ? ` · built ${ex.built}` : ''}</span>
						{/if}
					</button>
				{/each}
			{/if}
		</div>

		{#if active}
			<div class="eb-details" data-testid="examples-details">
				<h3 class="eb-details-title">{active.name}</h3>
				{#if active.features?.length}
					<ul class="eb-features">
						{#each active.features as f}
							<li>{f}</li>
						{/each}
					</ul>
				{/if}
				{#if active.tabs?.length}
					<div class="eb-row">Tabs: {active.tabs.join(', ')}</div>
				{/if}
				{#if active.generator}
					<div class="eb-row">
						Generator: <a class="eb-link" href={exampleGeneratorUrl(active)} download={active.generator} data-testid="example-generator-link">{active.generator}</a>
					</div>
				{/if}
				{#if active.built_with}
					<div class="eb-row eb-muted">{active.built_with}</div>
				{/if}
			</div>
		{/if}

		{#if state.writable}
			<div class="eb-save" data-testid="examples-save">
				{#if showSave}
					<input class="eb-input" type="text" placeholder="Example name" bind:value={saveName} data-testid="examples-save-name" />
					<input class="eb-input" type="text" placeholder="One-line description" bind:value={saveDescription} data-testid="examples-save-description" />
					<div class="eb-save-actions">
						<button class="eb-btn" onclick={() => (showSave = false)}>Cancel</button>
						<button class="eb-btn primary" disabled={state.saving || !saveName.trim()} data-testid="examples-save-submit" onclick={submitSave}>
							{state.saving ? 'Saving…' : 'Save'}
						</button>
					</div>
				{:else}
					<button class="eb-btn" data-testid="examples-save-current" onclick={openSave}>Save current as example</button>
					<span class="eb-muted">writes app/static/examples/ (dev server)</span>
				{/if}
			</div>
		{/if}
	</div>
</div>
{/if}

<style>
	.examples-browser {
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

	.eb-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border-bottom: 1px solid var(--border-color);
	}

	.eb-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary);
	}

	.eb-count {
		font-size: 11px;
		color: var(--text-secondary);
		background: var(--bg-primary);
		padding: 1px 6px;
		border-radius: 8px;
	}

	.eb-header-actions {
		margin-left: auto;
		display: flex;
		gap: 4px;
	}

	.eb-icon-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 14px;
		padding: 2px 4px;
		border-radius: 3px;
	}

	.eb-icon-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.eb-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.eb-list {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.eb-empty {
		padding: 20px 10px;
		text-align: center;
		color: var(--text-muted);
	}

	.eb-error {
		color: var(--error);
	}

	.eb-item {
		display: flex;
		flex-direction: column;
		width: 100%;
		padding: 8px 10px;
		border: none;
		border-bottom: 1px solid var(--border-color);
		background: none;
		cursor: pointer;
		text-align: left;
		color: var(--text-primary);
		font-size: 12px;
		gap: 2px;
	}

	.eb-item:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.eb-item:disabled {
		cursor: progress;
		opacity: 0.7;
	}

	.eb-item.active {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		border-left: 2px solid var(--accent);
	}

	.eb-item-row {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.eb-item-name {
		font-weight: 600;
		color: var(--accent);
	}

	.eb-badge {
		display: inline-block;
		font-size: 9px;
		font-weight: 700;
		padding: 1px 4px;
		border-radius: 3px;
		font-family: monospace;
		letter-spacing: 0.5px;
		line-height: 1.4;
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		color: var(--accent);
	}

	.eb-item-desc {
		font-size: 11px;
		color: var(--text-secondary);
		white-space: normal;
	}

	.eb-item-meta {
		font-size: 10px;
		color: var(--text-muted);
		font-style: italic;
	}

	.eb-details {
		padding: 8px 10px;
		border-top: 1px solid var(--border-color);
		background: var(--bg-tertiary);
		font-size: 11px;
		max-height: 45%;
		overflow-y: auto;
	}

	.eb-details-title {
		font-size: 12px;
		font-weight: 600;
		margin: 0 0 4px;
		color: var(--text-primary);
	}

	.eb-features {
		margin: 0 0 6px;
		padding-left: 16px;
		color: var(--text-secondary);
	}

	.eb-row {
		color: var(--text-secondary);
		padding: 1px 0;
		word-break: break-word;
	}

	.eb-muted {
		color: var(--text-muted);
		font-size: 10px;
	}

	.eb-link {
		color: var(--accent);
	}

	.eb-save {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 10px;
		border-top: 1px solid var(--border-color);
	}

	.eb-input {
		width: 100%;
		padding: 4px 6px;
		font-size: 11px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		outline: none;
	}

	.eb-input:focus {
		border-color: var(--accent);
	}

	.eb-save-actions {
		display: flex;
		gap: 6px;
		justify-content: flex-end;
	}

	.eb-btn {
		padding: 3px 8px;
		font-size: 11px;
		border: 1px solid var(--border-color);
		border-radius: 3px;
		background: var(--bg-primary);
		color: var(--text-primary);
		cursor: pointer;
	}

	.eb-btn.primary {
		background: var(--accent);
		color: var(--bg-primary);
		border-color: var(--accent);
	}

	.eb-btn:disabled {
		opacity: 0.5;
		cursor: default;
	}

	@media (max-width: 768px) {
		.examples-browser {
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

		.eb-header {
			flex-wrap: wrap;
			justify-content: center;
			cursor: grab;
		}

		.eb-header::before {
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
