<script>
	/**
	 * `/dev/icons` — every icon in the registry at toolbar (16 px), tile
	 * (20 px) and inspection (32 px) size, in the default, active and disabled
	 * button colours of the current theme (ui-chrome M13,
	 * `specs/ui_toolbar_icons.md`). A review page for the glyphs; switch theme
	 * with the picker to check light and dark.
	 */
	import Icon from '$lib/ui/icons/Icon.svelte';
	import ThemeSwitcher from '$lib/ui/ThemeSwitcher.svelte';
	import { ICON_NAMES } from '$lib/ui/icons/icons.js';
</script>

<svelte:head><title>Icon gallery</title></svelte:head>

<main class="gallery" data-testid="icon-gallery">
	<header>
		<h1>Icon gallery</h1>
		<span>{ICON_NAMES.length} names</span>
		<ThemeSwitcher />
	</header>
	<div class="grid">
		{#each ICON_NAMES as name (name)}
			<figure data-testid="icon-cell-{name}">
				<div class="row">
					<span class="state"><Icon {name} /></span>
					<span class="state"><Icon {name} size={20} /></span>
					<span class="state big"><Icon {name} size={32} /></span>
				</div>
				<div class="row">
					<span class="state active"><Icon {name} size={20} /></span>
					<span class="state disabled"><Icon {name} size={20} /></span>
				</div>
				<figcaption>{name}</figcaption>
			</figure>
		{/each}
	</div>
</main>

<style>
	.gallery {
		height: 100vh;
		overflow-y: auto;
		padding: 16px;
		box-sizing: border-box;
		background: var(--bg-primary);
		color: var(--text-primary);
		font-family: system-ui, sans-serif;
	}
	header {
		display: flex;
		align-items: center;
		gap: 16px;
		margin-bottom: 16px;
	}
	h1 {
		font-size: 18px;
		margin: 0;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: 12px;
	}
	figure {
		margin: 0;
		padding: 8px;
		border: 1px solid var(--border-color, var(--text-muted));
		border-radius: 6px;
		background: var(--bg-secondary);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 6px;
	}
	.state.active {
		color: var(--accent);
	}
	.state.disabled {
		color: var(--text-muted);
		opacity: 0.5;
	}
	figcaption {
		font-size: 11px;
		color: var(--text-secondary);
		word-break: break-all;
	}
</style>
