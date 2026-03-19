<script>
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { getStore } from '$lib/storage/index.js';
	import { onMount } from 'svelte';

	const id = $derived($page.params.id);

	onMount(async () => {
		const store = getStore();
		const doc = await store.get(id);
		if (doc) {
			sessionStorage.setItem('waffle-active-doc', doc.id);
			sessionStorage.setItem('waffle-active-json', doc.json);
		}
		goto('/', { replaceState: true });
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
