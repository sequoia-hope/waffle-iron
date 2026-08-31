<script>
	import { onMount } from 'svelte';
	import {
		getDimensionPopup,
		hideDimensionPopup,
		applyDimensionFromPopup,
		evaluateExpression,
		getSketchMode,
		getCameraObject,
		getDocumentDisplayUnit
	} from '$lib/engine/store.svelte.js';
	import { showToast } from '$lib/ui/toast.svelte.js';
	import { buildSketchPlane, sketchToScreen } from './sketchCoords.js';
	import { formatForInput, parseAndConvert, isPlainMeasurement } from '$lib/units.js';

	let popup = $derived(getDimensionPopup());
	let sm = $derived(getSketchMode());
	let displayUnit = $derived(getDocumentDisplayUnit());

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

	// When popup appears, set the default value (converted to display units) and focus
	$effect(() => {
		if (popup) {
			inputValue = formatForInput(popup.defaultValue, displayUnit);
			// Focus after DOM update
			requestAnimationFrame(() => {
				if (inputEl) {
					inputEl.focus();
					inputEl.select();
				}
			});
		}
	});

	async function handleKeyDown(e) {
		e.stopPropagation();
		if (e.key === 'Enter') {
			if (isPlainMeasurement(inputValue)) {
				const internalVal = parseAndConvert(inputValue, displayUnit);
				if (!isNaN(internalVal) && internalVal > 0) {
					applyDimensionFromPopup(internalVal);
				} else {
					hideDimensionPopup();
				}
				return;
			}
			// Not a plain number: try it as an expression over the design
			// variables (mm-space: bare numbers are mm). The popup's dims are
			// all lengths (distance/radius), so convert mm -> meters.
			const typed = inputValue.trim();
			if (typed) {
				const { value, error } = await evaluateExpression(typed);
				if (error == null && value != null && value * 0.001 > 0) {
					applyDimensionFromPopup(value * 0.001, typed);
					return;
				}
				showToast('error', `Dimension expression: ${error ?? 'must evaluate to a positive value'}`);
			}
			hideDimensionPopup();
		} else if (e.key === 'Escape') {
			hideDimensionPopup();
		}
	}

	function handleBlur() {
		// Auto-dismiss on blur without applying
		hideDimensionPopup();
	}

	const getSafeInset = (prop) =>
		parseFloat(getComputedStyle(document.documentElement).getPropertyValue(prop)) || 0;
</script>

{#if popup && screenPos}
	<div
		class="dimension-input-overlay"
		style="left: {Math.max(48 + getSafeInset('--sai-left'), Math.min(screenPos.x, window.innerWidth - 48 - getSafeInset('--sai-right')))}px; top: {Math.max(40, Math.min(screenPos.y, window.innerHeight - 16))}px;"
	>
		<input
			type="text"
			inputmode="decimal"
			class="dimension-input"
			bind:this={inputEl}
			bind:value={inputValue}
			onkeydown={handleKeyDown}
			onblur={handleBlur}
			placeholder={displayUnit}
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
