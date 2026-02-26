<script>
	import {
		getBooleanDialogState,
		hideBooleanDialog,
		applyBoolean
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getBooleanDialogState());
	let operation = $state('Union');
	let targetFeatureId = $state('');
	let toolFeatureId = $state('');

	$effect(() => {
		if (dialogState) {
			operation = 'Union';
			targetFeatureId = dialogState.bodies.length > 0 ? dialogState.bodies[0].featureId : '';
			toolFeatureId = '';
		}
	});

	let toolBodies = $derived(
		dialogState ? dialogState.bodies.filter(b => b.featureId !== targetFeatureId) : []
	);

	let canApply = $derived(targetFeatureId && toolFeatureId && targetFeatureId !== toolFeatureId);

	// Listen for keydown at window level so Escape works even without focus
	$effect(() => {
		if (!dialogState) return;
		function onKeyDown(e) {
			if (e.key === 'Enter') {
				e.preventDefault();
				e.stopPropagation();
				if (canApply) handleApply();
			} else if (e.key === 'Escape') {
				e.preventDefault();
				e.stopPropagation();
				handleCancel();
			}
		}
		window.addEventListener('keydown', onKeyDown, { capture: true });
		return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
	});

	function handleApply() {
		applyBoolean(operation, targetFeatureId, toolFeatureId)
			.catch(err => log('error', `Boolean dialog apply failed: ${err}`));
	}

	function handleCancel() {
		hideBooleanDialog();
	}

	function handleKeydown(e) {
		if (e.key === 'Enter') {
			e.preventDefault();
			if (canApply) handleApply();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			handleCancel();
		}
	}
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="boolean-panel" onkeydown={handleKeydown} data-testid="boolean-dialog">
		<div class="dialog-header">
			<span class="dialog-title">Boolean Combine</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			<div class="field">
				<span class="field-label">Operation</span>
				<div class="radio-group">
					<label class="radio-option">
						<input type="radio" bind:group={operation} value="Union" />
						Union
					</label>
					<label class="radio-option">
						<input type="radio" bind:group={operation} value="Subtract" />
						Subtract
					</label>
					<label class="radio-option">
						<input type="radio" bind:group={operation} value="Intersect" />
						Intersect
					</label>
				</div>
			</div>
			<div class="field">
				<label for="boolean-target">Target body</label>
				<select id="boolean-target" data-testid="boolean-target" bind:value={targetFeatureId}>
					<option value="" disabled>Select target...</option>
					{#each dialogState.bodies as body}
						<option value={body.featureId}>{body.name}</option>
					{/each}
				</select>
			</div>
			<div class="field">
				<label for="boolean-tool">Tool body</label>
				<select id="boolean-tool" data-testid="boolean-tool" bind:value={toolFeatureId}>
					<option value="" disabled>Select tool...</option>
					{#each toolBodies as body}
						<option value={body.featureId}>{body.name}</option>
					{/each}
				</select>
			</div>
		</div>
		<div class="dialog-footer">
			<button class="btn btn-cancel" data-testid="boolean-cancel" onclick={handleCancel}>Cancel</button>
			<button class="btn btn-apply" data-testid="boolean-apply" disabled={!canApply} onclick={handleApply}>Apply</button>
		</div>
	</div>
{/if}

<style>
	.boolean-panel {
		position: absolute;
		top: 12px;
		right: 12px;
		width: 240px;
		z-index: 50;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		pointer-events: auto;
	}

	@media (max-width: 768px) {
		.boolean-panel {
			position: fixed;
			top: auto;
			right: 0;
			bottom: 0;
			left: 0;
			width: 100%;
			max-height: 60vh;
			border-radius: 12px 12px 0 0;
			overflow-y: auto;
			z-index: 150;
			padding-bottom: env(safe-area-inset-bottom, 0px);
		}
	}

	.dialog-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-color, #444);
	}

	.dialog-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary, #eee);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 18px;
		cursor: pointer;
		padding: 0 2px;
		line-height: 1;
	}

	.close-btn:hover {
		color: var(--text-primary, #eee);
	}

	.dialog-body {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.field label, .field-label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
	}

	.radio-group {
		display: flex;
		gap: 10px;
	}

	.radio-option {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--text-primary, #eee);
		cursor: pointer;
	}

	.radio-option input[type="radio"] {
		margin: 0;
		accent-color: var(--accent, #0078d4);
	}

	.field select {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 8px;
		border-radius: 3px;
		font-size: 12px;
		width: 100%;
	}

	.field select:focus {
		outline: none;
		border-color: var(--accent, #0078d4);
	}

	.dialog-footer {
		display: flex;
		justify-content: flex-end;
		gap: 6px;
		padding: 8px 12px;
		border-top: 1px solid var(--border-color, #444);
	}

	.btn {
		padding: 5px 14px;
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
		border: 1px solid transparent;
	}

	.btn-cancel {
		background: transparent;
		color: var(--text-secondary, #aaa);
		border-color: var(--border-color, #444);
	}

	.btn-cancel:hover {
		background: var(--bg-hover, #333);
	}

	.btn-apply {
		background: var(--accent, #0078d4);
		color: #fff;
		border-color: var(--accent, #0078d4);
	}

	.btn-apply:hover:not(:disabled) {
		filter: brightness(1.1);
	}

	.btn-apply:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
