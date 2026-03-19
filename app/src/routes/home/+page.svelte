<script>
	import { goto } from '$app/navigation';
	import { getStore, migrateLocalStorage, generateDocId } from '$lib/storage/index.js';
	import { onMount } from 'svelte';
	import HomeHeader from '$lib/ui/HomeHeader.svelte';
	import DocumentGrid from '$lib/ui/DocumentGrid.svelte';

	let documents = $state([]);
	let loading = $state(true);

	onMount(async () => {
		const store = getStore();
		await migrateLocalStorage(store);
		documents = await store.list();
		loading = false;
	});

	async function handleNewDocument() {
		const store = getStore();
		const id = generateDocId();
		const now = Date.now();
		const tabId = crypto.randomUUID();
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
		await store.put(doc);
		goto('/');
	}

	function handleSelect(doc) {
		goto(`/doc/${doc.id}`);
	}
</script>

<div class="home-page" data-testid="home-page">
	<HomeHeader oncreate={handleNewDocument} />

	{#if loading}
		<div class="loading-area">
			<p>Loading documents...</p>
		</div>
	{:else}
		<DocumentGrid {documents} onselect={handleSelect} />
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
