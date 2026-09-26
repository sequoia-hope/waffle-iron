<script>
	/**
	 * Board data on hover (specs/kicad_board_link.md §1 goal 4, C4): a small
	 * card beside the pointer for a body a linked KiCad board derived — the
	 * component's reference, value and footprint, or the board's title. The
	 * store decides WHAT (one `QueryEntityMeta` per hovered body, cached);
	 * this component only places it. Click opens the detail panel.
	 */
	import { getEntityCard } from '$lib/engine/store.svelte.js';

	let card = $derived(getEntityCard());

	const getSafeInset = (prop) =>
		parseFloat(getComputedStyle(document.documentElement).getPropertyValue(prop)) || 0;

	// Beside the pointer, never under it, clamped to the window.
	let pos = $derived.by(() => {
		if (!card) return { x: 0, y: 0 };
		const w = 240;
		const h = 72;
		const maxX = window.innerWidth - w - 8 - getSafeInset('--sai-right');
		const maxY = window.innerHeight - h - 8;
		return {
			x: Math.min(card.x + 14, Math.max(getSafeInset('--sai-left'), maxX)),
			y: Math.min(card.y + 14, Math.max(0, maxY))
		};
	});

	let component = $derived(card?.meta?.component ?? null);
	let board = $derived(card?.meta?.board ?? null);
</script>

{#if card && (component || board)}
	<div
		class="kicad-card"
		data-testid="kicad-hover-card"
		role="tooltip"
		aria-live="polite"
		style="left: {pos.x}px; top: {pos.y}px"
	>
		{#if component}
			<div class="title">
				<span class="ref" data-testid="kicad-card-reference">{component.reference}</span>
				<span class="value">{component.value}</span>
			</div>
			<div class="line mono">{component.footprint}</div>
			<div class="line dim">{component.side} · {component.pads.length} pads · click for details</div>
		{:else}
			<div class="title">
				<span class="ref" data-testid="kicad-card-board">{board.title || 'Board'}</span>
				{#if board.rev}<span class="value">rev {board.rev}</span>{/if}
			</div>
			<div class="line dim">
				{board.copper_layers} layers · {(board.thickness_m * 1000).toFixed(2)} mm · {board.footprint_count} footprints
			</div>
		{/if}
	</div>
{/if}

<style>
	.kicad-card {
		position: fixed;
		z-index: 150;
		pointer-events: none;
		max-width: 240px;
		padding: 6px 9px;
		border-radius: 6px;
		border: 1px solid var(--border-color, #45475a);
		background: var(--bg-secondary, #181825);
		color: var(--text-primary, #cdd6f4);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
		font-size: 12px;
		line-height: 1.35;
	}
	.title {
		display: flex;
		gap: 8px;
		align-items: baseline;
		font-weight: 600;
	}
	.ref {
		font-size: 13px;
	}
	.value {
		font-weight: 400;
		color: var(--text-secondary, #a6adc8);
	}
	.line {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.mono {
		font-family: monospace;
		font-size: 11px;
	}
	.dim {
		color: var(--text-secondary, #a6adc8);
		font-size: 11px;
	}
</style>
