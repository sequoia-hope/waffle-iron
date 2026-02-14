<script>
	import {
		getExtrudeDialogState,
		hideExtrudeDialog,
		applyExtrude
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getExtrudeDialogState());
	let depth = $state(10);
	let profileIndex = $state(0);
	let cut = $state(false);

	$effect(() => {
		if (dialogState) {
			depth = 10;
			profileIndex = 0;
			cut = false;
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
		applyExtrude(depth, profileIndex, cut)
			.catch(err => log('error', `Extrude dialog apply failed: ${err}`));
	}

	function handleCancel() {
		hideExtrudeDialog();
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
	<div class="overlay" onkeydown={handleKeydown} data-testid="extrude-dialog">
		<div class="dialog">
			<div class="dialog-header">
				<span class="dialog-title">Extrude</span>
				<button class="close-btn" onclick={handleCancel}>&times;</button>
			</div>
			<div class="dialog-body">
				<div class="field">
					<label for="extrude-sketch">Sketch</label>
					<span id="extrude-sketch" class="field-value">{dialogState.sketchName}</span>
				</div>
				<div class="field">
					<label for="extrude-depth">Depth</label>
					<input
						id="extrude-depth"
						data-testid="extrude-depth"
						type="number"
						bind:value={depth}
						step="1"
						min="0.1"
					/>
				</div>
				<div class="field">
					<label for="extrude-cut">Cut</label>
					<input
						id="extrude-cut"
						data-testid="extrude-cut"
						type="checkbox"
						bind:checked={cut}
					/>
				</div>
			{#if dialogState.profileCount > 1}
					<div class="field">
						<label for="extrude-profile">Profile</label>
						<select id="extrude-profile" bind:value={profileIndex}>
							{#each Array(dialogState.profileCount) as _, i}
								<option value={i}>Profile {i + 1}</option>
							{/each}
						</select>
					</div>
				{/if}
			</div>
			<div class="dialog-footer">
				<button class="btn btn-cancel" data-testid="extrude-cancel" onclick={handleCancel}>Cancel</button>
				<button class="btn btn-apply" data-testid="extrude-apply" onclick={handleApply}>Apply</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		z-index: 1000;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(0, 0, 0, 0.3);
	}

	.dialog {
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		min-width: 280px;
		max-width: calc(100vw - 32px);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
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
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.field label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		min-width: 50px;
	}

	.field-value {
		font-size: 12px;
		color: var(--text-primary, #eee);
	}

	.field input[type="number"],
	.field select {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 8px;
		border-radius: 3px;
		font-size: 12px;
		width: 120px;
	}

	.field input[type="checkbox"] {
		width: auto;
		accent-color: var(--accent, #0078d4);
	}

	.field input:focus,
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

	.btn-apply:hover {
		filter: brightness(1.1);
	}
</style>
