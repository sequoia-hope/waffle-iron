<script>
	import { getSketchPlaneSelectionMode, exitSketchPlaneSelection } from '$lib/engine/store.svelte.js';

	let visible = $derived(getSketchPlaneSelectionMode());
</script>

{#if visible}
	<div class="sketch-plane-prompt" data-testid="sketch-plane-prompt">
		<span class="prompt-text">Select a sketch plane — click a plane in the viewport or feature tree</span>
		<button class="cancel-btn" data-testid="sketch-plane-prompt-cancel" onclick={() => exitSketchPlaneSelection()}>Cancel</button>
	</div>
{/if}

<style>
	.sketch-plane-prompt {
		position: absolute;
		top: 8px;
		left: 50%;
		transform: translateX(-50%);
		z-index: 50;
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--accent, #0078d4);
		border-radius: 6px;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
	}

	.prompt-text {
		font-size: 13px;
		color: var(--text-primary, #eee);
		white-space: nowrap;
	}

	.cancel-btn {
		background: transparent;
		border: 1px solid var(--border-color, #444);
		color: var(--text-secondary, #aaa);
		padding: 4px 12px;
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
	}

	.cancel-btn:hover {
		background: var(--bg-hover, #333);
		color: var(--text-primary, #eee);
	}

	@media (max-width: 768px) {
		.sketch-plane-prompt {
			left: max(8px, env(safe-area-inset-left, 0px));
			right: max(8px, env(safe-area-inset-right, 0px));
			transform: none;
			flex-wrap: wrap;
			text-align: center;
			justify-content: center;
		}

		.prompt-text {
			white-space: normal;
		}
	}
</style>
