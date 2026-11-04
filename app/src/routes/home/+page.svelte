<script>
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { getActiveProvider, migrateLocalStorage, getStore, registerProvider, unregisterProvider, setActiveProvider } from '$lib/storage/index.js';
	import { newDocumentRecord } from '$lib/storage/newDocument.js';
	import { editDocumentMeta } from '$lib/engine/format.js';
	import { onMount } from 'svelte';
	import HomeHeader from '$lib/ui/HomeHeader.svelte';
	import DocumentGrid from '$lib/ui/DocumentGrid.svelte';

	let documents = $state([]);
	let loading = $state(true);
	/** Last share link produced (shown briefly; also copied to the clipboard). */
	let shareNotice = $state('');
	/** Shown when a rename on the home page conflicts with another tab's write. */
	let conflictNotice = $state('');
	let canShare = $derived.by(() => { void documents; return !!getActiveProvider()?.canShare; });

	async function handleShare(doc) {
		const provider = getActiveProvider();
		const url = await provider.getShareUrl?.(doc.id);
		if (!url) return;
		shareNotice = url;
		try {
			await navigator.clipboard.writeText(url);
		} catch {
			// Clipboard may be unavailable; the link is shown instead.
		}
		setTimeout(() => { if (shareNotice === url) shareNotice = ''; }, 8000);
	}

	/** @type {(() => void) | null} */
	let unwatchProvider = null;

	function installProviderWatch() {
		unwatchProvider?.();
		unwatchProvider =
			getActiveProvider()?.watch?.(() => {
				refreshDocuments();
			}) ?? null;
	}

	onMount(() => {
		installProviderWatch();
		(async () => {
			// Migrate legacy localStorage autosave (always to local provider)
			await migrateLocalStorage(getStore());
			await refreshDocuments();
		})();
		return () => {
			unwatchProvider?.();
			unwatchProvider = null;
		};
	});

	async function refreshDocuments() {
		loading = true;
		try {
			const provider = getActiveProvider();
			documents = await provider.list();
		} catch (err) {
			console.warn('Failed to list documents:', err);
			documents = [];
		}
		loading = false;
	}

	async function handleNewDocument() {
		const provider = getActiveProvider();
		const doc = newDocumentRecord();
		const res = await provider.put(doc, { base: null });
		if ('conflict' in res) {
			await refreshDocuments();
			return;
		}
		goto(`${base}/doc/${doc.id}`);
	}

	function handleSelect(doc) {
		goto(`${base}/doc/${doc.id}`);
	}

	async function handleRename(doc, newName) {
		const provider = getActiveProvider();
		const found = await provider.get(doc.id);
		if (!found) return;
		try {
			// A TARGETED edit of the metadata object, not a parse/stringify of
			// the whole file: a stored document carries persistent entity ids,
			// and a pre-v10 file carries them as JSON numbers, which a
			// JavaScript round trip rounds above 2^53 into different entities
			// (`editDocumentMeta`, `waffle_types::pid_str`). Renaming a
			// document used to silently unanchor every drawing dimension and
			// entity name in it.
			const updated = {
				...found.doc,
				json: editDocumentMeta(found.doc.json, (meta) => {
					meta.name = newName;
				}),
				modified: Date.now()
			};
			const res = await provider.put(updated, { base: found.rev });
			if ('conflict' in res) {
				console.warn(`Rename of ${doc.id} conflicted with revision ${res.conflict.theirs_rev}`);
				conflictNotice = `Could not rename “${doc.name || 'Untitled'}” because it was modified in another tab or window. Showing the latest stored version.`;
			} else {
				conflictNotice = '';
			}
			documents = await provider.list();
		} catch (err) {
			console.warn('Failed to rename document:', err);
			conflictNotice = `Failed to rename “${doc.name || 'Untitled'}”: ${err?.message || err}`;
		}
	}

	/**
	 * Download the stored record's `.waffle` text exactly as stored, without
	 * opening the document: no engine load, no rebuild, no autosave can touch
	 * it first. Also the way to get a document out that no longer opens.
	 */
	async function handleExport(doc) {
		const found = await getActiveProvider().get(doc.id);
		if (!found?.doc?.json) return;
		const blob = new Blob([found.doc.json], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `${doc.name || 'document'}.waffle`;
		document.body.appendChild(a);
		a.click();
		document.body.removeChild(a);
		URL.revokeObjectURL(url);
	}

	async function handleDelete(doc) {
		if (!confirm(`Delete "${doc.name}"? This cannot be undone.`)) return;
		const provider = getActiveProvider();
		await provider.delete(doc.id);
		documents = await provider.list();
	}

	function handleProviderChange() {
		installProviderWatch();
		refreshDocuments();
	}
</script>

<div class="home-page" data-testid="home-page">
	<HomeHeader oncreate={handleNewDocument} onproviderchange={handleProviderChange} />
	{#if shareNotice}
		<p class="share-notice" data-testid="share-notice">Share link copied: <code>{shareNotice}</code></p>
	{/if}
	{#if conflictNotice}
		<div class="conflict-notice" data-testid="home-conflict-notice" role="alert">
			<span>{conflictNotice}</span>
			<button
				type="button"
				class="conflict-notice-dismiss"
				data-testid="home-conflict-notice-dismiss"
				onclick={() => { conflictNotice = ''; }}
			>
				Dismiss
			</button>
		</div>
	{/if}

	{#if loading}
		<div class="loading-area">
			<p>Loading documents...</p>
		</div>
	{:else}
		<DocumentGrid {documents} onselect={handleSelect} onrename={handleRename} ondelete={handleDelete} onshare={canShare ? handleShare : null} onexport={handleExport} />
	{/if}
</div>

<style>
	.share-notice {
		margin: 8px 32px 0;
		font-size: 12px;
		color: var(--text-secondary, #a6adc8);
		word-break: break-all;
	}

	.conflict-notice {
		margin: 12px 32px 0;
		padding: 10px 14px;
		border-radius: 6px;
		border: 1px solid var(--error, #f38ba8);
		background: rgba(243, 139, 168, 0.12);
		color: var(--text-primary, #cdd6f4);
		font-size: 13px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.conflict-notice-dismiss {
		background: transparent;
		border: 1px solid var(--border-color, #45475a);
		border-radius: 4px;
		color: var(--text-primary, #cdd6f4);
		font-size: 12px;
		padding: 3px 10px;
		cursor: pointer;
		flex-shrink: 0;
	}

	.conflict-notice-dismiss:hover {
		background: var(--bg-hover, #313244);
	}

	.home-page {
		height: 100vh;
		height: 100dvh;
		background: var(--bg-primary, #1e1e2e);
		color: var(--text-primary, #cdd6f4);
		display: flex;
		flex-direction: column;
		overflow-y: auto;
	}

	.loading-area {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-secondary, #a6adc8);
	}
</style>
