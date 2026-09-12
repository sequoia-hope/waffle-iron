<script>
	import '../app.css';
	import { onMount } from 'svelte';
	import { initEngine } from '$lib/engine/store.svelte.js';
	import { initTheme } from '$lib/ui/theme.svelte.js';
	import { initSettings } from '$lib/ui/settings.svelte.js';
	import '$lib/storage/git-init.js';

	let { children } = $props();

	onMount(() => {
		initTheme();
		initSettings();
		initEngine();
	});
</script>

{@render children()}
