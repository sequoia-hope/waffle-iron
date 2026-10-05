<script>
	/**
	 * The place-view dialog (`specs/drawings_and_mbd.md` §8, D4e): what a view
	 * IS — its source, its direction and its scale — asked once, before the
	 * sheet enters placement mode and the cursor says WHERE it goes.
	 *
	 * Two steps rather than one because the two halves answer to different
	 * inputs: the source and the scale are typed, and the placement is pointed
	 * at. A dialog that also took the placement would be the numeric panel
	 * again; a tool that took the source from the pointer would have to invent
	 * a way to point at a tab.
	 *
	 * It owns no engine call. `onplace` hands the caller the three answers and
	 * the sheet does the rest — which is what keeps this component usable from
	 * the panel today and from D4d's toolbar at merge, with no second copy of
	 * the flow.
	 */
	import { DRAWING_NAMED_VIEWS } from '$lib/engine/store.svelte.js';
	import { ISO_5455_SCALES } from '$lib/drawings/viewPlacement.js';

	let {
		/** The Part and Assembly tabs this drawing can draw: `{ id, name }`. */
		sources = [],
		/** Called with `{ sourceTab, view, scale }` when OK is pressed. */
		onplace = () => {},
		oncancel = () => {}
	} = $props();

	let sourceTab = $state(sources[0]?.id ?? '');
	let namedView = $state('Front');
	/** The chosen series entry, or `'free'`. */
	let scaleChoice = $state('1');
	let freeScale = $state(1);

	let scale = $derived(scaleChoice === 'free' ? Number(freeScale) : Number(scaleChoice));
	let scaleOk = $derived(Number.isFinite(scale) && scale > 0);
	/**
	 * Why a typed scale is refused, in words (D4e review).
	 *
	 * A disabled OK is a refusal the user has to GUESS at, and `<input
	 * type="number">` hands back an empty string for anything it could not
	 * parse — so "3x", a pasted "1:2" and a blank field all arrive here
	 * identically and all used to just grey the button out. Saying it also
	 * covers the two values that parse and are still not scales: zero, and a
	 * negative ratio that would mirror the view.
	 */
	let scaleRefusal = $derived(
		scaleChoice !== 'free' || scaleOk
			? null
			: !String(freeScale ?? '').trim() || !Number.isFinite(Number(freeScale))
				? 'A ratio is a number: 2 for 2:1, 0.5 for 1:2.'
				: `A scale must be greater than zero, not ${Number(freeScale)}.`
	);

	function place() {
		if (!sourceTab || !scaleOk) return;
		onplace({ sourceTab, view: namedView, scale });
	}

	function keydown(event) {
		// Escape cancels from anywhere in the dialog, which is the same key that
		// cancels the placement mode it opens — one gesture for "not this".
		if (event.key === 'Escape') {
			event.stopPropagation();
			oncancel();
		}
		if (event.key === 'Enter' && sourceTab && scaleOk) {
			event.stopPropagation();
			place();
		}
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
	class="dialog"
	role="dialog"
	aria-label="Place a view"
	data-testid="dwg-place-view-dialog"
	tabindex="-1"
	onkeydown={keydown}
>
	<div class="title">Place a view</div>
	<label>
		part
		<select data-testid="dwg-place-source" bind:value={sourceTab} disabled={!sources.length}>
			{#each sources as t (t.id)}
				<option value={t.id}>{t.name}</option>
			{/each}
		</select>
	</label>
	<label>
		view
		<select data-testid="dwg-place-view-direction" bind:value={namedView}>
			{#each DRAWING_NAMED_VIEWS as v}
				<option value={v}>{v.toLowerCase()}</option>
			{/each}
		</select>
	</label>
	<label title="The ISO 5455 series. A scale off the series is a drawing a reader cannot check against a scale rule — but 'other' is there, because a 3:1 detail is a thing people draw.">
		scale
		<select data-testid="dwg-place-scale" bind:value={scaleChoice}>
			{#each ISO_5455_SCALES as s}
				<option value={String(s.scale)}>{s.ratio}</option>
			{/each}
			<option value="free">other…</option>
		</select>
	</label>
	{#if scaleChoice === 'free'}
		<label title="Paper length per model length: 1 is 1:1, 0.5 is 1:2">
			ratio
			<input
				class="num"
				type="number"
				step="0.1"
				min="0.0001"
				data-testid="dwg-place-scale-free"
				bind:value={freeScale}
			/>
		</label>
	{/if}
	{#if scaleRefusal}
		<div class="warn" data-testid="dwg-place-scale-refusal">{scaleRefusal}</div>
	{/if}
	{#if !sources.length}
		<div class="warn" data-testid="dwg-place-no-source">
			This document has no Part or Assembly tab to draw.
		</div>
	{/if}
	<div class="actions">
		<button
			class="act primary"
			data-testid="dwg-place-ok"
			disabled={!sourceTab || !scaleOk}
			onclick={place}>OK</button
		>
		<button class="act" data-testid="dwg-place-cancel" onclick={() => oncancel()}>Cancel</button>
	</div>
	<div class="hint">Then click the sheet where the view goes. Escape cancels.</div>
</div>

<style>
	/* Sized relative to the window and scrolling rather than overflowing, the
	   rule every panel and dialog in the chrome follows (CLAUDE.md, "Chrome
	   must scroll or collapse, never overflow"). */
	.dialog {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px;
		box-sizing: border-box;
		width: min(260px, 90vw);
		max-height: min(420px, 80vh);
		overflow-y: auto;
		background: var(--bg-secondary, #fff);
		border: 1px solid var(--border-color, #ccc);
		border-radius: 4px;
		font-size: 12px;
		color: var(--text-primary);
	}

	.title {
		font-weight: 600;
	}

	label {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		color: var(--text-secondary);
		font-size: 11px;
	}

	select,
	.num {
		flex: 1 1 60px;
		min-width: 60px;
		max-width: 100%;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		font-size: 11px;
		padding: 1px 2px;
	}

	.actions {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
	}

	.act {
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		font-size: 11px;
		padding: 2px 8px;
		cursor: pointer;
	}

	.act.primary {
		border-color: var(--color-accent, var(--border-color));
	}

	.act:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.hint,
	.warn {
		font-size: 11px;
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}

	.warn {
		color: var(--color-warning, #b80);
	}
</style>
