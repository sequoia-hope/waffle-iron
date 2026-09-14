<script>
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { getActiveProvider, migrateLocalStorage, getStore, registerProvider, unregisterProvider, setActiveProvider } from '$lib/storage/index.js';
	import { newDocumentRecord } from '$lib/storage/newDocument.js';
	import { onMount } from 'svelte';
	import HomeHeader from '$lib/ui/HomeHeader.svelte';
	import DocumentGrid from '$lib/ui/DocumentGrid.svelte';

	let documents = $state([]);
	let loading = $state(true);
	/** Last share link produced (shown briefly; also copied to the clipboard). */
	let shareNotice = $state('');
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

	onMount(async () => {
		// Migrate legacy localStorage autosave (always to local provider)
		await migrateLocalStorage(getStore());
		await refreshDocuments();
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
		await provider.put(doc);
		goto(`${base}/doc/${doc.id}`);
	}

	function handleSelect(doc) {
		goto(`${base}/doc/${doc.id}`);
	}

	async function handleRename(doc, newName) {
		const provider = getActiveProvider();
		const stored = await provider.get(doc.id);
		if (!stored) return;
		try {
			const parsed = JSON.parse(stored.json);
			if (parsed.document) {
				parsed.document.name = newName;
			} else if (parsed.project) {
				parsed.project.name = newName;
			}
			stored.json = JSON.stringify(parsed);
			stored.modified = Date.now();
			await provider.put(stored);
			documents = await provider.list();
		} catch { /* ignore */ }
	}

	async function handleDelete(doc) {
		if (!confirm(`Delete "${doc.name}"? This cannot be undone.`)) return;
		const provider = getActiveProvider();
		await provider.delete(doc.id);
		documents = await provider.list();
	}

	function handleProviderChange() {
		refreshDocuments();
	}
</script>

<div class="home-page" data-testid="home-page">
	<HomeHeader oncreate={handleNewDocument} onproviderchange={handleProviderChange} />
	{#if shareNotice}
		<p class="share-notice" data-testid="share-notice">Share link copied: <code>{shareNotice}</code></p>
	{/if}

	{#if loading}
		<div class="loading-area">
			<p>Loading documents...</p>
		</div>
	{:else}
		<DocumentGrid {documents} onselect={handleSelect} onrename={handleRename} ondelete={handleDelete} onshare={canShare ? handleShare : null} />
	{/if}
</div>

<style>
	.share-notice {
		margin: 8px 32px 0;
		font-size: 12px;
		color: var(--text-secondary, #a6adc8);
		word-break: break-all;
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
