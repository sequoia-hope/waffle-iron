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
	import { renderSheetSvg, sheetExtentMm } from '$lib/drawings/sheet.js';
	// D4e. The two placement tools' mode lives in its own module (the tool is
	// started in the panel, or in D4d's toolbar, and runs here), so what the
	// sheet owns is the pointer events and nothing else.
	import {
		cancelPlacement,
		placementGhostSvg,
		placementMode,
		placementPointerDown,
		placementPointerMove
	} from '$lib/drawings/placementMode.svelte.js';
	import { paperPointMm, withGhost } from '$lib/drawings/viewPlacement.js';

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
	let mode = $derived(placementMode());
	/** The ghost is spliced INTO the sheet's markup rather than overlaid: in
	 *  the sheet's own user units it cannot be out by a pixel, where an
	 *  absolutely positioned overlay would have to re-measure the CSS-scaled
	 *  paper on every frame. */
	let markup = $derived(
		rendered ? withGhost(rendered.svg, mode ? placementGhostSvg(sheet) : '') : null
	);

	/** @type {HTMLDivElement | null} */
	let host = null;

	/** The `<svg class="wi-sheet">` inside the injected markup. */
	function sheetEl() {
		return host?.querySelector('svg.wi-sheet') ?? null;
	}

	/**
	 * The paper point under a pointer event, in sheet millimetres from the
	 * BOTTOM-left corner.
	 *
	 * `getScreenCTM()` when the browser gives one — the element's own mapping
	 * from its user units (paper mm, y DOWN) to the screen, so any letterboxing
	 * `preserveAspectRatio` introduces is already in it, and D4d's anchor hit
	 * test reads its nested view `<svg>` the same way. `paperPointMm`'s
	 * rect-and-viewBox ratio is the fallback for a context with no CTM (jsdom,
	 * a detached node), and it agrees with the CTM exactly while the sheet's
	 * `width`/`height` in mm match its `viewBox`, which is how `sheet.js`
	 * writes it.
	 */
	function pointAt(event) {
		const el = sheetEl();
		if (!el || !sheet) return null;
		const [, heightMm] = sheetExtentMm(sheet);
		const ctm = el.getScreenCTM?.();
		if (ctm && typeof el.createSVGPoint === 'function') {
			const p = el.createSVGPoint();
			p.x = event.clientX;
			p.y = event.clientY;
			const user = p.matrixTransform(ctm.inverse());
			// The one flip: user units measure y down from the top, a placement
			// measures it up from the bottom (`sheet.js`'s own expression).
			if (Number.isFinite(user.x) && Number.isFinite(user.y)) {
				return [user.x, heightMm - user.y];
			}
		}
		return paperPointMm(el.getBoundingClientRect(), event, sheetExtentMm(sheet));
	}

	function move(event) {
		if (!mode) return;
		const at = pointAt(event);
		if (at) placementPointerMove(at);
	}

	function down(event) {
		if (!mode) return;
		const at = pointAt(event);
		if (!at) return;
		// The sheet swallows the click while a tool is running: a placement
		// click must not also reach whatever is under it.
		event.preventDefault();
		placementPointerDown(at);
	}

	function keydown(event) {
		if (mode && event.key === 'Escape') {
			event.stopPropagation();
			cancelPlacement();
		}
	}
</script>

<svelte:window onkeydown={keydown} />

<!-- The wrapper scrolls rather than overflows: an A3 sheet at 1:1 is wider
     than the viewport at most window sizes, and the page itself cannot scroll
     (CLAUDE.md, "Chrome must scroll or collapse, never overflow"). -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="drawing-sheet"
	class:placing={!!mode}
	data-testid="drawing-sheet"
	data-views={rendered?.views ?? 0}
	data-placement-mode={mode ?? ''}
	bind:this={host}
	onpointermove={move}
	onpointerdown={down}
>
	{#if rendered}
		{@html markup}
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

	/* A placement tool is running: the cursor says so, and the paper does not
	   select under the drag. */
	.drawing-sheet.placing {
		cursor: crosshair;
		user-select: none;
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
