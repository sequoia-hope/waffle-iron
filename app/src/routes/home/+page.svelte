<script>
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { getActiveProvider, migrateLocalStorage, generateDocId, getStore, registerProvider, unregisterProvider, setActiveProvider } from '$lib/storage/index.js';
	import { onMount } from 'svelte';
	import HomeHeader from '$lib/ui/HomeHeader.svelte';
	import DocumentGrid from '$lib/ui/DocumentGrid.svelte';

	let documents = $state([]);
	let loading = $state(true);

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
		const id = generateDocId();
		const now = Date.now();
		const tabId = (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function')
			? crypto.randomUUID()
			: ([1e7]+-1e3+-4e3+-8e3+-1e11).replace(/[018]/g, c =>
				(c ^ crypto.getRandomValues(new Uint8Array(1))[0] & 15 >> c / 4).toString(16));
		const doc = {
			id,
			json: JSON.stringify({
				format: 'waffle-iron',
				version: 3,
				document: {
					name: 'Untitled',
					created: new Date(now).toISOString(),
					modified: new Date(now).toISOString()
				},
				tabs: [{
					id: tabId,
					name: 'Part 1',
					kind: { type: 'Part', features: { features: [], active_index: null } }
				}],
				active_tab: tabId
			}),
			created: now,
			modified: now
		};
		await provider.put(doc);
		goto(`${base}/doc/${id}`);
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

	{#if loading}
		<div class="loading-area">
			<p>Loading documents...</p>
		</div>
	{:else}
		<DocumentGrid {documents} onselect={handleSelect} onrename={handleRename} ondelete={handleDelete} />
	{/if}
</div>

<style>
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
