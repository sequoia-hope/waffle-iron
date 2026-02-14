<script>
	import { onMount } from 'svelte';
	import {
		getDimensionPopup,
		hideDimensionPopup,
		applyDimensionFromPopup,
		getSketchMode,
		getCameraObject
	} from '$lib/engine/store.svelte.js';
	import { buildSketchPlane, sketchToScreen } from './sketchCoords.js';

	let popup = $derived(getDimensionPopup());
	let sm = $derived(getSketchMode());

	let inputValue = $state('');
	/** @type {HTMLInputElement | null} */
	let inputEl = null;

	// Compute screen position from sketch coordinates
	let screenPos = $derived.by(() => {
		if (!popup || !sm?.active) return null;
		const camera = getCameraObject();
		if (!camera) return null;
		const canvas = /** @type {HTMLCanvasElement} */ (document.querySelector('canvas'));
		if (!canvas) return null;
		const plane = buildSketchPlane(sm.origin, sm.normal);
		return sketchToScreen(popup.sketchX, popup.sketchY, plane, camera, canvas);
	});

	// When popup appears, set the default value and focus the input
	$effect(() => {
		if (popup) {
			inputValue = String(popup.defaultValue);
			// Focus after DOM update
			requestAnimationFrame(() => {
				if (inputEl) {
					inputEl.focus();
					inputEl.select();
				}
			});
		}
	});

	function handleKeyDown(e) {
		e.stopPropagation();
		if (e.key === 'Enter') {
			const val = parseFloat(inputValue);
			if (!isNaN(val) && val > 0) {
				applyDimensionFromPopup(val);
			} else {
				hideDimensionPopup();
			}
		} else if (e.key === 'Escape') {
			hideDimensionPopup();
		}
	}

	function handleBlur() {
		// Auto-dismiss on blur without applying
		hideDimensionPopup();
	}
</script>

{#if popup && screenPos}
	<div
		class="dimension-input-overlay"
		style="left: {screenPos.x}px; top: {screenPos.y}px;"
	>
		<input
			type="number"
			class="dimension-input"
			bind:this={inputEl}
			bind:value={inputValue}
			onkeydown={handleKeyDown}
			onblur={handleBlur}
			step="any"
		/>
	</div>
{/if}

<style>
	.dimension-input-overlay {
		position: fixed;
		z-index: 1100;
		transform: translate(-50%, -100%) translateY(-8px);
		pointer-events: auto;
	}

	.dimension-input {
		background: rgba(30, 30, 50, 0.95);
		color: #ffffff;
		border: 1px solid #44cc88;
		border-radius: 3px;
		padding: 3px 8px;
		font-size: 12px;
		font-family: monospace;
		width: 80px;
		outline: none;
		text-align: center;
	}

	.dimension-input:focus {
		border-color: #66ddaa;
		box-shadow: 0 0 6px rgba(68, 204, 136, 0.3);
	}

	@media (max-width: 768px) {
		.dimension-input {
			font-size: 16px;
		}
	}
</style>
