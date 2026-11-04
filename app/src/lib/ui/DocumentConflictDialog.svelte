<script>
	import {
		getDocumentConflict,
		reloadAfterConflict,
		replaceConflictedDocument,
		saveConflictAsCopy
	} from '$lib/engine/store.svelte.js';

	let conflict = $derived(getDocumentConflict());
	let busy = $state(false);

	async function handleReload() {
		if (busy) return;
		busy = true;
		try {
			await reloadAfterConflict();
		} finally {
			busy = false;
		}
	}

	async function handleSaveCopy() {
		if (busy) return;
		busy = true;
		try {
			await saveConflictAsCopy();
		} finally {
			busy = false;
		}
	}

	async function handleReplace() {
		if (busy) return;
		busy = true;
		try {
			await replaceConflictedDocument();
		} finally {
			busy = false;
		}
	}
</script>

{#if conflict}
	<div class="overlay" data-testid="document-conflict-dialog" role="alertdialog" aria-modal="true" aria-labelledby="document-conflict-title">
		<div class="dialog">
			<div class="dialog-header">
				<span class="dialog-title" id="document-conflict-title">
					{conflict.fromImport ? 'Document Already Exists in Storage' : 'Document Changed Elsewhere'}
				</span>
			</div>
			<div class="dialog-body">
				<p class="message">
					{#if conflict.theirsRev === null}
						“{conflict.name || 'Untitled'}” was deleted in another tab or window while you were editing it.
					{:else if conflict.fromImport}
						A stored document with the same ID as “{conflict.name || 'Untitled'}” already exists in {conflict.providerLabel}.
					{:else}
						“{conflict.name || 'Untitled'}” was saved in another tab or window since this tab opened it.
					{/if}
				</p>
				<p class="hint">
					{#if conflict.fromImport}
						Saving was paused so the stored document is not silently overwritten. Load the stored version, save this import as a copy, or explicitly replace the stored document.
					{:else}
						Your save was refused so neither version is overwritten. Reload the latest stored version (discarding this tab’s unsaved edits) or save this tab’s work as a copy.
					{/if}
				</p>
			</div>
			<div class="dialog-footer">
				<button
					type="button"
					class="btn btn-secondary"
					data-testid="conflict-reload-btn"
					disabled={busy || conflict.theirsRev === null}
					onclick={handleReload}
				>
					Reload
				</button>
				{#if conflict.fromImport && conflict.theirsRev !== null}
					<button
						type="button"
						class="btn btn-danger"
						data-testid="conflict-replace-btn"
						disabled={busy}
						onclick={handleReplace}
					>
						Replace
					</button>
				{/if}
				<button
					type="button"
					class="btn btn-apply"
					data-testid="conflict-save-copy-btn"
					disabled={busy}
					onclick={handleSaveCopy}
				>
					Save as a copy
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		z-index: 1100;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(0, 0, 0, 0.45);
	}

	.dialog {
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		width: min(420px, calc(100vw - 32px - env(safe-area-inset-left, 0px) - env(safe-area-inset-right, 0px)));
		box-sizing: border-box;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.45);
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
		gap: 8px;
	}

	.message {
		font-size: 13px;
		color: var(--text-primary, #eee);
		margin: 0;
		line-height: 1.4;
		word-break: break-word;
	}

	.hint {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		margin: 0;
		line-height: 1.4;
	}

	.dialog-footer {
		display: flex;
		flex-wrap: wrap;
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

	.btn:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.btn-secondary {
		background: transparent;
		color: var(--text-primary, #eee);
		border-color: var(--border-color, #444);
	}

	.btn-secondary:hover:not(:disabled) {
		background: var(--bg-hover, #333);
	}

	.btn-danger {
		background: transparent;
		color: var(--error, #f38ba8);
		border-color: var(--error, #f38ba8);
	}

	.btn-danger:hover:not(:disabled) {
		background: rgba(243, 139, 168, 0.14);
	}

	.btn-apply {
		background: var(--accent, #0078d4);
		color: var(--text-on-accent, #fff);
		border-color: var(--accent, #0078d4);
	}

	.btn-apply:hover:not(:disabled) {
		filter: brightness(1.1);
	}
</style>
