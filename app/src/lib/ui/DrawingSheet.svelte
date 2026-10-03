<script>
	/**
	 * The drawing sheet (`specs/drawings_and_mbd.md` §8, D4a): the paper, with
	 * the sheet's views on it, where the 3D viewport sits on a Part tab.
	 *
	 * Thin, for the reason `DrawingView.svelte` is thin: all the drawing lives
	 * in `sheet.js`'s pure `renderSheetSvg`, so the markup a user sees, the
	 * markup `export_svg` writes and the markup a byte oracle hashes are the
	 * SAME STRING. `{@html}` is safe because every interpolated value goes
	 * through `svg.js`'s `esc`; no caller-supplied markup reaches the output.
	 *
	 * It reads the store rather than taking props, the way `AssemblyPanel`
	 * does: the open Drawing tab's evaluation is store state, and a second
	 * copy passed down would be the next thing to go stale.
	 */
	import { getDocumentDisplayUnit, getDrawingSheet, getDrawingStatus } from '$lib/engine/store.svelte.js';
	import { renderSheetSvg } from '$lib/drawings/sheet.js';

	let { sheetId = null } = $props();

	let status = $derived(getDrawingStatus());
	let sheet = $derived(getDrawingSheet(sheetId));
	let rendered = $derived(
		sheet
			? renderSheetSvg({
					sheet,
					unit: getDocumentDisplayUnit(),
					documentPrecision: 2
				})
			: null
	);
</script>

<!-- The wrapper scrolls rather than overflows: an A3 sheet at 1:1 is wider
     than the viewport at most window sizes, and the page itself cannot scroll
     (CLAUDE.md, "Chrome must scroll or collapse, never overflow"). -->
<div class="drawing-sheet" data-testid="drawing-sheet" data-views={rendered?.views ?? 0}>
	{#if rendered}
		{@html rendered.svg}
	{:else}
		<p class="empty" data-testid="drawing-sheet-empty">This drawing has no sheet.</p>
	{/if}
	{#if status?.errors?.length}
		<!-- A view that failed is NOT drawn, so the sheet looks finished
		     without it. The errors ride with the paper for that reason. -->
		<ul class="problems" data-testid="drawing-sheet-errors">
			{#each status.errors as e}
				<li>{e}</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.drawing-sheet {
		width: 100%;
		height: 100%;
		overflow: auto;
		background: var(--bg-secondary);
		padding: 12px;
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
	}

	/* The sheet carries its own mm size so printing is true to scale; on
	   screen it must not force the panel wider than the window. */
	.drawing-sheet :global(svg.wi-sheet) {
		max-width: 100%;
		height: auto;
		flex: 0 0 auto;
		box-shadow: 0 1px 6px rgba(0, 0, 0, 0.25);
	}

	.empty {
		color: var(--text-secondary);
		font-size: 12px;
	}

	.problems {
		margin: 0;
		padding: 6px 10px 6px 24px;
		max-width: 100%;
		font-size: 11px;
		color: var(--color-error, #c33);
		background: var(--bg-primary);
		border-radius: 4px;
		box-sizing: border-box;
		overflow-wrap: anywhere;
	}
</style>
