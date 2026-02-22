<script>
	import {
		getSaveTestCaseDialogState,
		hideSaveTestCaseDialog,
		saveAsTestCase
	} from '$lib/engine/store.svelte.js';

	let state = $derived(getSaveTestCaseDialogState());
	let saving = $state(false);

	let name = $state('');
	let description = $state('');
	let expectedOutcome = $state('should_pass');
	let tags = $state('');

	$effect(() => {
		if (state) {
			name = state.name;
			description = state.description;
			expectedOutcome = state.expectedOutcome;
			tags = state.tags;
		}
	});

	async function handleSave() {
		if (!name.trim()) return;
		saving = true;
		try {
			await saveAsTestCase(name.trim(), description.trim(), expectedOutcome, tags.trim());
		} finally {
			saving = false;
		}
	}

	function handleKeydown(e) {
		if (e.key === 'Enter' && !saving) handleSave();
		if (e.key === 'Escape') hideSaveTestCaseDialog();
	}
</script>

{#if state}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="stcd-backdrop" data-testid="save-test-case-backdrop" onkeydown={handleKeydown} onclick={() => hideSaveTestCaseDialog()}>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="stcd-dialog" data-testid="save-test-case-dialog" onclick={(e) => e.stopPropagation()} onkeydown={handleKeydown}>
		<div class="stcd-title">Save as Test Case</div>

		<label class="stcd-label">
			Name
			<input
				class="stcd-input"
				type="text"
				data-testid="stcd-name"
				bind:value={name}
				placeholder="Test case name"
			/>
		</label>

		<label class="stcd-label">
			Description
			<input
				class="stcd-input"
				type="text"
				data-testid="stcd-description"
				bind:value={description}
				placeholder="Optional description"
			/>
		</label>

		<label class="stcd-label">
			Expected Outcome
			<select class="stcd-input" data-testid="stcd-outcome" bind:value={expectedOutcome}>
				<option value="should_pass">Should pass</option>
				<option value="known_failure">Known failure</option>
				<option value="regression">Regression</option>
			</select>
		</label>

		<label class="stcd-label">
			Tags
			<input
				class="stcd-input"
				type="text"
				data-testid="stcd-tags"
				bind:value={tags}
				placeholder="comma, separated, tags"
			/>
		</label>

		<div class="stcd-actions">
			<button class="stcd-btn stcd-cancel" data-testid="stcd-cancel" onclick={() => hideSaveTestCaseDialog()}>Cancel</button>
			<button class="stcd-btn stcd-save" data-testid="stcd-save" disabled={!name.trim() || saving} onclick={handleSave}>
				{saving ? 'Saving...' : 'Save'}
			</button>
		</div>
	</div>
</div>
{/if}

<style>
	.stcd-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
	}

	.stcd-dialog {
		background: var(--bg-secondary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		padding: 16px;
		width: 340px;
		max-width: 90vw;
	}

	.stcd-title {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 12px;
	}

	.stcd-label {
		display: block;
		font-size: 11px;
		color: var(--text-secondary);
		margin-bottom: 8px;
	}

	.stcd-input {
		display: block;
		width: 100%;
		margin-top: 3px;
		padding: 5px 8px;
		font-size: 12px;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		outline: none;
		box-sizing: border-box;
	}

	.stcd-input:focus {
		border-color: var(--accent);
	}

	.stcd-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 14px;
	}

	.stcd-btn {
		padding: 5px 14px;
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
		border: 1px solid var(--border-color);
	}

	.stcd-cancel {
		background: var(--bg-primary);
		color: var(--text-primary);
	}

	.stcd-cancel:hover {
		background: var(--bg-hover);
	}

	.stcd-save {
		background: var(--accent);
		color: white;
		border-color: var(--accent);
	}

	.stcd-save:hover:not(:disabled) {
		opacity: 0.9;
	}

	.stcd-save:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
