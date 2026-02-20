<script>
	import {
		getAutoRestoreState,
		restoreAutoSave,
		discardAutoSave
	} from '$lib/engine/store.svelte.js';

	let state = $derived(getAutoRestoreState());

	function formatTimeAgo(timestamp) {
		const seconds = Math.floor((Date.now() - timestamp) / 1000);
		if (seconds < 60) return 'just now';
		const minutes = Math.floor(seconds / 60);
		if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'} ago`;
		const hours = Math.floor(minutes / 60);
		if (hours < 24) return `${hours} hour${hours === 1 ? '' : 's'} ago`;
		const days = Math.floor(hours / 24);
		return `${days} day${days === 1 ? '' : 's'} ago`;
	}

	async function handleRestore() {
		await restoreAutoSave();
	}

	function handleDiscard() {
		discardAutoSave();
	}
</script>

{#if state?.available}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="overlay" data-testid="auto-restore-dialog">
		<div class="dialog">
			<div class="dialog-header">
				<span class="dialog-title">Restore Unsaved Work?</span>
			</div>
			<div class="dialog-body">
				<p class="message">You have unsaved work from {formatTimeAgo(state.timestamp)}.</p>
				<p class="hint">Would you like to restore it?</p>
			</div>
			<div class="dialog-footer">
				<button class="btn btn-cancel" data-testid="auto-restore-discard" onclick={handleDiscard}>Discard</button>
				<button class="btn btn-apply" data-testid="auto-restore-restore" onclick={handleRestore}>Restore</button>
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
		min-width: 300px;
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

	.dialog-body {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.message {
		font-size: 13px;
		color: var(--text-primary, #eee);
		margin: 0;
	}

	.hint {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		margin: 0;
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
</style>
