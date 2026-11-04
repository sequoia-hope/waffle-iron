<script>
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { page } from '$app/stores';
	import { getStore, getActiveProvider } from '$lib/storage/index.js';
	import { onMount } from 'svelte';

	const id = $derived($page.params.id);

	onMount(async () => {
		// The active provider first (a document listed on the home page comes
		// from it), then the local store — the two are different stores.
		const active = getActiveProvider();
		let found = null;
		let foundProviderId = active.id;
		try {
			found = await active.get(id);
		} catch {
			found = null;
		}
		if (!found && active.id !== 'local') {
			const local = getStore();
			found = await local.get(id);
			foundProviderId = local.id;
		}
		if (found) {
			const { doc, rev } = found;
			sessionStorage.setItem('waffle-active-doc', doc.id);
			sessionStorage.setItem('waffle-active-json', doc.json);
			sessionStorage.setItem('waffle-active-rev', rev);
			sessionStorage.setItem('waffle-active-rev-provider', foundProviderId);
			// A linked (share-link) record stays read-only when reopened.
			if (doc.link) sessionStorage.setItem('waffle-active-link', JSON.stringify(doc.link));
			else sessionStorage.removeItem('waffle-active-link');
		}
		goto(`${base}/`, { replaceState: true });
	});
</script>

<div class="loading-page">
	<p>Loading document...</p>
</div>

<style>
	.loading-page {
		height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-secondary, #a6adc8);
		background: var(--bg-primary, #1e1e2e);
	}
</style>
