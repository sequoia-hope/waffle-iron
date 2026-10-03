<script>
	/**
	 * One annotated drawing view, as SVG (`specs/drawings_and_mbd.md` §7).
	 *
	 * Deliberately a thin wrapper. All the drawing lives in `svg.js`'s pure
	 * `renderViewSvg`, and this component only puts its output in the
	 * document — so the markup a user sees, the markup `export_svg` (D4a)
	 * writes, and the markup a byte oracle hashes are the SAME STRING. A
	 * second, declarative `{#each}` renderer would be a second source of
	 * truth for the same geometry, and the two would drift.
	 *
	 * The sheet itself — paper size, title block, several views placed on it
	 * — is D4a's `DrawingSheet.svelte`, which composes this.
	 *
	 * `{@html}` is safe here because the string is built by `svg.js`, which
	 * XML-escapes every value it interpolates (`esc`); no caller-supplied
	 * markup reaches the output.
	 */
	import { renderViewSvg } from './svg.js';

	let {
		/** A `ViewLayout`: `{ curves, bbox, annotations }`. */
		layout,
		/** Drawing scale as a ratio; 1 = 1:1. */
		scale = 1,
		/** Partial `DrawingStyle` overrides — the document-settings seam. */
		style = undefined,
		/** Display unit for dimension text. */
		unit = 'mm',
		documentPrecision = 2,
		title = null,
		/** Called with the render's warnings, if any. */
		onwarnings = undefined
	} = $props();

	let rendered = $derived(
		renderViewSvg({ layout, scale, style, unit, documentPrecision, title })
	);

	$effect(() => {
		if (rendered.warnings.length > 0) onwarnings?.(rendered.warnings);
	});
</script>

<!-- The wrapper scrolls rather than overflows: a 1:1 sheet is larger than the
     panel at most window sizes, and the page itself cannot scroll
     (CLAUDE.md, "Chrome must scroll or collapse, never overflow"). -->
<div class="drawing-view" data-testid="drawing-view">
	{@html rendered.svg}
</div>

<style>
	.drawing-view {
		width: 100%;
		height: 100%;
		overflow: auto;
		background: var(--bg-secondary);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding: 8px;
		box-sizing: border-box;
	}

	/* The SVG carries its own mm size so printing is true to scale; on screen
	   it must not force the panel wider than the window. */
	.drawing-view :global(svg.wi-drawing) {
		max-width: 100%;
		height: auto;
		flex: 0 0 auto;
	}
</style>
