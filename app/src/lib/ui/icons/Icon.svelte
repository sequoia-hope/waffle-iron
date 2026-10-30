<!--
  One icon from the registry in `icons.js` (ui-chrome M13,
  `specs/ui_toolbar_icons.md`). Fixed width/height attributes so the toolbar's
  synchronous collapse measurement (`contentFits()` in Toolbar.svelte) sees
  the final size on first layout; colour is `currentColor`, so the button's
  own colour (active, disabled, success, danger) and the theme drive it.
  Decorative: always `aria-hidden` — the button's label or aria-label names it.
-->
<script>
	import { iconMarkup } from './icons.js';

	/** @type {{ name: string, size?: number }} */
	let { name, size = 16 } = $props();

	let markup = $derived(iconMarkup(name));
</script>

{#if markup}
	<svg
		class="wi-icon"
		data-icon={name}
		width={size}
		height={size}
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="2"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
		focusable="false"
	>
		{@html markup}
	</svg>
{/if}

<style>
	.wi-icon {
		flex: none;
		display: inline-block;
		vertical-align: middle;
	}
	/* Construction / reference geometry. */
	.wi-icon :global(.c) {
		stroke-dasharray: 2.5 2.5;
		opacity: 0.55;
	}
	/* Axes and mirror lines. */
	.wi-icon :global(.axis) {
		stroke-dasharray: 4 2 1 2;
		opacity: 0.55;
	}
	/* Points. */
	.wi-icon :global(.p) {
		fill: currentColor;
		stroke: none;
	}
	/* Dimension and annotation strokes. */
	.wi-icon :global(.d) {
		stroke-width: 1.5;
	}
	/* Arrowheads. */
	.wi-icon :global(.a) {
		fill: currentColor;
		stroke: none;
	}
	/* Shaded faces and overlaps. */
	.wi-icon :global(.f) {
		fill: currentColor;
		stroke: none;
		opacity: 0.3;
	}
</style>
