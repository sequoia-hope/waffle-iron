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
	import { formatForInput, parseAndConvert, isPlainMeasurement, UNIT_ORDER, UNITS } from '$lib/units.js';

	let popup = $derived(getDimensionPopup());
	let sm = $derived(getSketchMode());
	let displayUnit = $derived(getDocumentDisplayUnit());

	let inputValue = $state('');
	// The value the popup opened with. Blur commits only when the entry
	// differs from it: tapping away from an untouched popup still cancels,
	// but a typed value survives the iOS keypad's Done (✓) — that keypad
	// has no Return key, and Done only blurs the field.
	let initialValue = '';
	// Unit this entry is read in. Defaults to the document unit; the unit
	// chip cycles it per dimension, so a number keypad can still enter
	// inches into a mm document (the stored value is meters either way).
	let entryUnit = $state('mm');
	let committing = false;
	/** @type {HTMLInputElement | null} */
	let inputEl = null;

	// Compute screen position from sketch coordinates
	let screenPos = $derived.by(() => {
		if (!popup || !sm?.active) return null;
		const camera = getCameraObject();
		if (!camera) return null;
		const canvas = /** @type {HTMLCanvasElement} */ (document.querySelector('canvas'));
		if (!canvas) return null;
		const plane = buildSketchPlane(sm.origin, sm.normal, sm.xAxis);
		return sketchToScreen(popup.sketchX, popup.sketchY, plane, camera, canvas);
	});

	// When popup appears, set the default value (converted to display units) and focus
	$effect(() => {
		if (popup) {
			// Local first: reading inputValue here would make every keystroke
			// re-run this effect and reset the field.
			const initial = popup.angle
				? String(parseFloat(Number(popup.defaultValue).toFixed(4)))
				: formatForInput(popup.defaultValue, displayUnit);
			inputValue = initial;
			initialValue = initial;
			entryUnit = displayUnit;
			committing = false;
			// Focus after DOM update
			requestAnimationFrame(() => {
				if (inputEl) {
					inputEl.focus();
					inputEl.select();
				}
			});
		}
	});

	async function commit() {
		if (!popup || committing) return;
		// Closing the popup unmounts the input, which blurs it; the guard
		// keeps that blur (or one during the expression await) from
		// committing twice.
		committing = true;
		if (popup.angle) {
			// Degrees: no length-unit conversion. Expressions evaluate in degrees.
			const deg = parseFloat(inputValue);
			if (/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(inputValue.trim()) && !isNaN(deg) && deg > 0) {
				applyDimensionFromPopup(deg);
				return;
			}
			const typedAngle = inputValue.trim();
			if (typedAngle) {
				const { value, error } = await evaluateExpression(typedAngle);
				if (error == null && value != null && value > 0) {
					applyDimensionFromPopup(value, typedAngle);
					return;
				}
				showToast('error', `Dimension expression: ${error ?? 'must evaluate to a positive angle'}`);
			}
			hideDimensionPopup();
			return;
		}
		if (isPlainMeasurement(inputValue)) {
			const internalVal = parseAndConvert(inputValue, entryUnit);
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
	}

	function handleKeyDown(e) {
		e.stopPropagation();
		if (e.key === 'Enter') {
			commit();
		} else if (e.key === 'Escape') {
			committing = true;
			hideDimensionPopup();
		}
	}

	function handleBlur() {
		if (committing) return;
		if (inputValue.trim() !== initialValue || entryUnit !== displayUnit) {
			commit();
		} else {
			// Untouched: tapping/clicking away cancels.
			hideDimensionPopup();
		}
	}

	function cycleEntryUnit() {
		const i = UNIT_ORDER.indexOf(entryUnit);
		entryUnit = UNIT_ORDER[(i + 1) % UNIT_ORDER.length];
	}

	const getSafeInset = (prop) =>
		parseFloat(getComputedStyle(document.documentElement).getPropertyValue(prop)) || 0;
</script>

{#if popup && screenPos}
	<div
		class="dimension-input-overlay"
		style="left: {Math.max(48 + getSafeInset('--sai-left'), Math.min(screenPos.x, window.innerWidth - 48 - getSafeInset('--sai-right')))}px; top: {Math.max(40, Math.min(screenPos.y, window.innerHeight - 16))}px;"
	>
		{#if popup.prefix}
			<span class="dimension-prefix" data-testid="dimension-input-prefix">{popup.prefix}</span>
		{/if}
		<input
			type="text"
			inputmode="decimal"
			class="dimension-input"
			bind:this={inputEl}
			bind:value={inputValue}
			onkeydown={handleKeyDown}
			onblur={handleBlur}
			placeholder={popup.angle ? 'deg' : entryUnit}
		/>
		{#if popup.angle}
			<span class="dimension-unit" data-testid="dimension-input-unit">{'\u00B0'}</span>
		{:else}
			<!-- preventDefault on pointerdown keeps focus (and the keypad) in
			     the input while the unit cycles. -->
			<button
				type="button"
				class="dimension-unit dimension-unit-toggle"
				class:dimension-unit-override={entryUnit !== displayUnit}
				data-testid="dimension-input-unit"
				title="Unit for this value (tap to change)"
				onpointerdown={(e) => e.preventDefault()}
				onmousedown={(e) => e.preventDefault()}
				onclick={cycleEntryUnit}
			>{UNITS[entryUnit]?.label ?? entryUnit}</button>
		{/if}
	</div>
{/if}

<style>
	.dimension-input-overlay {
		position: fixed;
		z-index: 1100;
		transform: translate(-50%, -100%) translateY(-8px);
		pointer-events: auto;
		display: flex;
		align-items: center;
		gap: 4px;
		background: color-mix(in srgb, var(--bg-secondary) 96%, transparent);
		border: 1px solid var(--sketch-constrained);
		border-radius: 3px;
		padding: 2px 6px;
		font-size: 12px;
		font-family: monospace;
		color: var(--text-primary);
	}
	.dimension-prefix { color: var(--sketch-constrained); }
	.dimension-unit { color: var(--text-muted); }
	.dimension-unit-toggle {
		background: transparent;
		border: 1px solid var(--border-color, currentColor);
		border-radius: 3px;
		font: inherit;
		padding: 0 4px;
		cursor: pointer;
		touch-action: manipulation;
	}
	.dimension-unit-override {
		color: var(--sketch-snap);
		border-color: var(--sketch-snap);
	}

	.dimension-input {
		background: transparent;
		color: var(--text-primary);
		border: none;
		padding: 1px 2px;
		font-size: 12px;
		font-family: monospace;
		width: 72px;
		outline: none;
		text-align: center;
	}

	.dimension-input-overlay:focus-within {
		border-color: var(--sketch-snap);
		box-shadow: 0 0 6px color-mix(in srgb, var(--sketch-snap) 40%, transparent);
	}

	@media (max-width: 768px) {
		.dimension-input {
			font-size: 16px;
		}
		.dimension-unit-toggle {
			font-size: 16px;
			min-height: 32px;
			min-width: 40px;
		}
	}
</style>
