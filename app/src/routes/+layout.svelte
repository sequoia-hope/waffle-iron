<script>
	import '../app.css';
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { page } from '$app/stores';
	import { get } from 'svelte/store';
	import { initEngine } from '$lib/engine/store.svelte.js';
	import { initTheme } from '$lib/ui/theme.svelte.js';
	import { initSettings } from '$lib/ui/settings.svelte.js';
	import '$lib/storage/git-init.js';

	let { children } = $props();

	onMount(() => {
		initTheme();
		initSettings();
		// The viewer (`/view`, specs/waffle_server_mode.md §4.10) never starts
		// the engine worker and never downloads the wasm: it draws what a host
		// computes.
		const pathname = get(page).url.pathname;
		if (!pathname.startsWith(`${base}/view`)) initEngine();
	});
</script>

{@render children()}
