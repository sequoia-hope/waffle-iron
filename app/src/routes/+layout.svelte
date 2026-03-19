<script>
	import '../app.css';
	import { onMount } from 'svelte';
	import { initEngine } from '$lib/engine/store.svelte.js';
	import '$lib/storage/github-init.js';

	let { children } = $props();

	onMount(() => {
		initEngine();
	});
</script>

{@render children()}
