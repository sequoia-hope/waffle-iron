<script>
	/**
	 * "Link STEP": import a STEP file by link as a LINKED source
	 * (specs/waffle_v4_document_model.md §2.3, Phase 2 P2-3). Accepts a
	 * GitHub/GitLab/Gitea file URL, a raw URL, an `/open` share link, or any
	 * https URL; the document records the locator + commit instead of a copy.
	 */
	import { getImportLinkDialogState, hideImportLinkDialog, importStepFromLink, linkKicadFromLink } from '$lib/engine/store.svelte.js';

	let dialog = $derived(getImportLinkDialogState());
	// 'step' (the original) or 'kicad' (specs/kicad_board_link.md C4): the
	// same dialog, a different expectation of what the link points at.
	let kind = $derived(dialog?.kind ?? 'step');
	let copy = $derived(
		kind === 'kicad'
			? {
					title: 'Link a KiCad board',
					hint: 'Paste a link to a .kicad_pcb on GitHub, GitLab or Gitea. The board outline becomes an exact solid, its footprints an assembly, and the document keeps the link and the commit — not a copy.',
					placeholder: 'https://github.com/owner/repo/blob/main/hardware/board.kicad_pcb'
				}
			: {
					title: 'Link a STEP file',
					hint: 'Paste a file link from GitHub, GitLab or Gitea (or a share link). The document keeps the link and the commit it was fetched at — not a copy.',
					placeholder: 'https://github.com/owner/repo/blob/main/parts/bracket.step'
				}
	);
	let url = $state('');
	let busy = $state(false);

	$effect(() => {
		if (dialog) url = dialog.url ?? '';
	});

	async function submit(e) {
		e?.preventDefault?.();
		if (!url.trim() || busy) return;
		busy = true;
		try {
			const ok = kind === 'kicad' ? await linkKicadFromLink(url.trim()) : await importStepFromLink(url.trim());
			if (ok) hideImportLinkDialog();
		} finally {
			busy = false;
		}
	}
</script>

{#if dialog}
	<div class="overlay" data-testid="import-link-dialog" data-kind={kind}>
		<form class="panel" onsubmit={submit}>
			<h3>{copy.title}</h3>
			<p class="hint">{copy.hint}</p>
			<input
				data-testid="import-link-url"
				type="url"
				placeholder={copy.placeholder}
				bind:value={url}
				autocomplete="off"
			/>
			<div class="actions">
				<button type="button" class="btn" data-testid="import-link-cancel" onclick={hideImportLinkDialog} disabled={busy}>Cancel</button>
				<button type="submit" class="btn btn-apply" data-testid="import-link-submit" disabled={busy || !url.trim()}>
					{busy ? 'Fetching…' : 'Link'}
				</button>
			</div>
		</form>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(0, 0, 0, 0.45);
		z-index: 200;
	}
	.panel {
		width: min(520px, 90vw);
		padding: 20px;
		border-radius: 8px;
		border: 1px solid var(--border-color, #45475a);
		background: var(--bg-secondary, #181825);
		color: var(--text-primary, #cdd6f4);
	}
	h3 {
		margin: 0 0 8px;
		font-size: 15px;
	}
	.hint {
		font-size: 12px;
		color: var(--text-secondary, #a6adc8);
		margin: 0 0 12px;
	}
	input {
		width: 100%;
		box-sizing: border-box;
		padding: 8px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #45475a);
		background: var(--bg-primary, #1e1e2e);
		color: inherit;
		font-family: monospace;
		font-size: 12px;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 12px;
	}
	.btn {
		padding: 6px 12px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #45475a);
		background: transparent;
		color: inherit;
		cursor: pointer;
	}
	.btn-apply {
		background: var(--accent, #89b4fa);
		color: var(--bg-primary, #1e1e2e);
		border-color: transparent;
	}
	.btn:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
