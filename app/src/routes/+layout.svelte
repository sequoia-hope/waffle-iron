<script>
	import '../app.css';
	import { onMount } from 'svelte';
	import { initEngine } from '$lib/engine/store.svelte.js';
	import { initTheme } from '$lib/ui/theme.svelte.js';
	import '$lib/storage/github-init.js';

	let { children } = $props();

	onMount(() => {
		initTheme();
		initEngine();
	});
</script>

{@render children()}
