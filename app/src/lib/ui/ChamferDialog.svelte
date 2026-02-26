<script>
	import {
		getChamferDialogState,
		hideChamferDialog,
		applyChamfer
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getChamferDialogState());
	let distance = $state(1.0);

	$effect(() => {
		if (dialogState) {
			distance = 1.0;
		}
	});

	// Listen for keydown at window level so Escape works even without focus
	$effect(() => {
		if (!dialogState) return;
		function onKeyDown(e) {
			if (e.key === 'Enter') {
				e.preventDefault();
				e.stopPropagation();
				handleApply();
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
		applyChamfer(distance)
			.catch(err => log('error', `Chamfer dialog apply failed: ${err}`));
	}

	function handleCancel() {
		hideChamferDialog();
	}

	function handleKeydown(e) {
		if (e.key === 'Enter') {
			e.preventDefault();
			handleApply();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			handleCancel();
		}
	}
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="chamfer-panel" onkeydown={handleKeydown} data-testid="chamfer-dialog">
		<div class="dialog-header">
			<span class="dialog-title">Chamfer</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			<div class="deferred-warning" data-testid="chamfer-deferred-warning">
				Chamfer is not yet available. Edge selection and kernel support are under development.
			</div>
			<div class="info-row">
				<span class="info-label">Selected edges</span>
				<span class="info-value" data-testid="chamfer-edge-count">{dialogState.edgeCount}</span>
			</div>
			<div class="field">
				<label for="chamfer-distance">Distance</label>
				<input
					id="chamfer-distance"
					data-testid="chamfer-distance"
					type="number"
					bind:value={distance}
					step="0.1"
					min="0.01"
				/>
			</div>
		</div>
		<div class="dialog-footer">
			<button class="btn btn-cancel" data-testid="chamfer-cancel" onclick={handleCancel}>Cancel</button>
			<button class="btn btn-apply" data-testid="chamfer-apply" disabled onclick={handleApply}>Apply</button>
		</div>
	</div>
{/if}

<style>
	.chamfer-panel {
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
		.chamfer-panel {
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

	.deferred-warning {
		padding: 8px 10px;
		background: rgba(255, 170, 0, 0.12);
		border: 1px solid rgba(255, 170, 0, 0.3);
		border-radius: 4px;
		font-size: 11px;
		color: var(--text-secondary, #aaa);
		line-height: 1.4;
	}

	.dialog-body {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.info-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.info-label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
	}

	.info-value {
		font-size: 12px;
		color: var(--text-primary, #eee);
		font-weight: 600;
	}

	.field {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.field label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		min-width: 50px;
	}

	.field input[type="number"] {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 8px;
		border-radius: 3px;
		font-size: 12px;
		width: 120px;
	}

	.field input:focus {
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
