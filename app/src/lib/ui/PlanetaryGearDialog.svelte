<script>
	import {
		getPlanetaryDialogOpen,
		getPlanetaryDialogState,
		hidePlanetaryDialog,
		createPlanetary,
		getMobileLayout,
		getDocumentDisplayUnit,
		getBridge
	} from '$lib/engine/store.svelte.js';
	import { setPreview } from '$lib/sketch/sketchToolState.svelte.js';
	import { log } from '$lib/engine/logger.js';
	import { internalToDisplay, parseAndConvert, UNITS } from '$lib/units.js';
	import { DEFAULT_GEAR_MODULE_DISPLAY, DEFAULT_GEAR_PRESSURE_ANGLE } from '$lib/config.js';

	let isOpen = $derived(getPlanetaryDialogOpen());
	let dialogState = $derived(getPlanetaryDialogState());
	// Placement center (internal sketch coords) — already internal, NO unit
	// conversion (the click position is in sketch space, not a display input).
	let centerX = $derived(dialogState?.centerX ?? 0);
	let centerY = $derived(dialogState?.centerY ?? 0);
	let isMobile = $derived(getMobileLayout());
	let displayUnit = $derived(getDocumentDisplayUnit());
	let unitLabel = $derived(UNITS[displayUnit]?.label ?? displayUnit);

	let moduleInput = $state(DEFAULT_GEAR_MODULE_DISPLAY.mm);
	let pressureAngle = $state(DEFAULT_GEAR_PRESSURE_ANGLE);
	let sunTeeth = $state(24);
	let planetTeeth = $state(16);
	let planetCount = $state(4);
	let backlashInput = $state(0);
	let autoAdjust = $state(false);

	let creating = $state(false);

	// Reset to defaults each time the dialog opens.
	$effect(() => {
		if (isOpen) {
			moduleInput = DEFAULT_GEAR_MODULE_DISPLAY[displayUnit] ?? DEFAULT_GEAR_MODULE_DISPLAY.mm;
			pressureAngle = DEFAULT_GEAR_PRESSURE_ANGLE;
			sunTeeth = 24;
			planetTeeth = 16;
			planetCount = 4;
			backlashInput = 0;
			autoAdjust = false;
			creating = false;
		}
	});

	// Convert module/backlash from the display unit to internal meters.
	// (The offset-plane dialog once skipped this and planes landed 1000× off —
	// do NOT repeat it.)
	let module_ = $derived(parseAndConvert(moduleInput, displayUnit) || 0);
	let backlash_ = $derived(parseAndConvert(String(backlashInput), displayUnit) || 0);

	// Live-derived values (mirror the Rust core's formulas).
	let zr = $derived(Math.round(sunTeeth) + 2 * Math.round(planetTeeth));
	let carrierRadius = $derived((Math.round(sunTeeth) + Math.round(planetTeeth)) * module_ / 2);
	let sum = $derived(Math.round(sunTeeth) + zr);

	const MIN_TEETH = 6;
	const MAX_PLANETS = 12;

	/** @returns {string[]} */
	function validNs() {
		const out = [];
		for (let d = 1; d <= MAX_PLANETS; d++) if (sum % d === 0) out.push(d);
		return out;
	}

	// Validation hints (mirror gear_planetary.rs::collect_hints + check_basics).
	let hints = $derived.by(() => {
		const out = [];
		const N = Math.round(planetCount);
		if (Math.round(sunTeeth) < MIN_TEETH) out.push(`Sun teeth must be at least ${MIN_TEETH}.`);
		if (Math.round(planetTeeth) < MIN_TEETH) out.push(`Planet teeth must be at least ${MIN_TEETH}.`);
		if (N < 1 || N > MAX_PLANETS) out.push(`Planet count must be 1–${MAX_PLANETS}.`);
		if (!(module_ > 0)) out.push('Module must be positive.');
		if (N >= 1 && N <= MAX_PLANETS && sum % N !== 0) {
			out.push(`(Z_s + Z_r) = ${sum} must be divisible by N. Valid planet counts: ${validNs().join(', ')}.`);
		}
		// Non-interference: r_p + module < R_c·sin(π/N).
		const rp = Math.round(planetTeeth) * module_ / 2;
		if (N >= 1 && module_ > 0 && rp + module_ >= carrierRadius * Math.sin(Math.PI / N)) {
			out.push(`${N} planets of ${Math.round(planetTeeth)} teeth collide — reduce N or increase sun teeth.`);
		}
		return out;
	});

	// In hint mode a blocking hint disables Create. In auto-adjust mode, basic
	// (non-snappable) errors still block, but assembly/interference will snap.
	let basicError = $derived(
		Math.round(sunTeeth) < MIN_TEETH ||
		Math.round(planetTeeth) < MIN_TEETH ||
		!(module_ > 0) ||
		Math.round(planetCount) < 1 || Math.round(planetCount) > MAX_PLANETS
	);
	let blocking = $derived(basicError || (!autoAdjust && hints.length > 0));

	// Build the engine params (camelCase serde) for preview + create. The center
	// is threaded straight through (already internal sketch coords).
	function buildParams() {
		return {
			module: module_,
			pressureAngleDeg: pressureAngle,
			sunTeeth: Math.round(sunTeeth),
			planetTeeth: Math.round(planetTeeth),
			planetCount: Math.round(planetCount),
			backlash: backlash_,
			centerX,
			centerY,
			autoAdjust
		};
	}

	// Live preview via WASM — latest-wins (generation counter), mirroring the
	// single-gear dialog. Updates as params/center change; cleared on close.
	let previewGeneration = 0;
	$effect(() => {
		if (!isOpen) {
			setPreview(null);
			return;
		}
		const params = buildParams();
		const gen = ++previewGeneration;
		const bridge = getBridge();
		if (!bridge) return;
		bridge.send({ type: 'GeneratePlanetaryPreview', params }).then(response => {
			if (gen === previewGeneration) {
				setPreview({ type: 'planetary-preview', data: { polylines: response.polylines } });
			}
		}).catch(() => { /* stale or bridge error — ignore */ });
	});

	$effect(() => {
		if (!isOpen) return;
		function onKeyDown(e) {
			if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); handleCreate(); }
			else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); handleCancel(); }
		}
		window.addEventListener('keydown', onKeyDown, { capture: true });
		return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
	});

	async function handleCreate() {
		if (blocking || creating) return;
		creating = true;
		const res = await createPlanetary(buildParams());
		creating = false;
		if (res) {
			log('sketch', `Planetary stage: ${res.result.gears.length} gears`);
			setPreview(null);
			hidePlanetaryDialog();
		}
		// On null (blocked/invalid), keep the dialog open; createPlanetary toasts.
	}

	function handleCancel() { setPreview(null); hidePlanetaryDialog(); }
</script>

{#if isOpen}
	<div class="planetary-dialog" class:mobile={isMobile} data-testid="planetary-dialog">
		<div class="dialog-header">
			<span class="dialog-title">Planetary Gear Stage</span>
			<button class="close-btn" onclick={handleCancel} data-testid="planetary-dialog-close">&times;</button>
		</div>

		<div class="dialog-body">
			<div class="param-row">
				<label for="pl-module">Module ({unitLabel})</label>
				<input id="pl-module" type="text" inputmode="decimal" bind:value={moduleInput}
					data-testid="planetary-module-input" />
			</div>

			<div class="param-row">
				<label for="pl-pressure">Pressure Angle</label>
				<input id="pl-pressure" type="number" min="14.5" max="30" step="0.5"
					bind:value={pressureAngle} data-testid="planetary-pressure-input" />
			</div>

			<div class="param-row">
				<label for="pl-sun">Sun Teeth</label>
				<input id="pl-sun" type="number" min="6" step="1" bind:value={sunTeeth}
					data-testid="planetary-sun-input" />
			</div>

			<div class="param-row">
				<label for="pl-planet">Planet Teeth</label>
				<input id="pl-planet" type="number" min="6" step="1" bind:value={planetTeeth}
					data-testid="planetary-planet-input" />
			</div>

			<div class="param-row">
				<label for="pl-count">Planet Count (N)</label>
				<input id="pl-count" type="number" min="1" max="12" step="1" bind:value={planetCount}
					data-testid="planetary-count-input" />
			</div>

			<div class="param-row">
				<label for="pl-backlash">Backlash ({unitLabel})</label>
				<input id="pl-backlash" type="number" min="0" step="0.01" bind:value={backlashInput}
					data-testid="planetary-backlash-input" />
			</div>

			<div class="param-row">
				<label for="pl-auto">Auto-adjust N</label>
				<input id="pl-auto" type="checkbox" bind:checked={autoAdjust}
					data-testid="planetary-autoadjust-input"
					title="When on, snap an invalid planet count to the nearest valid divisor. When off, an invalid stage blocks with a hint." />
			</div>

			<div class="divider"></div>

			<div class="param-row">
				<label>Ring Teeth (Z_s+2·Z_p)</label>
				<span class="derived-value" data-testid="planetary-ring-teeth">{zr}</span>
			</div>
			<div class="param-row">
				<label>Carrier Radius</label>
				<span class="derived-value" data-testid="planetary-carrier-radius">
					{internalToDisplay(carrierRadius, displayUnit).toFixed(3)} {unitLabel}
				</span>
			</div>

			{#if hints.length > 0}
				<div class="hints" data-testid="planetary-hints" class:blocking={!autoAdjust}>
					{#each hints as h}<div class="hint">{h}</div>{/each}
					{#if autoAdjust && !basicError}<div class="hint info">Will auto-adjust on Create.</div>{/if}
				</div>
			{/if}
		</div>

		<div class="dialog-footer">
			<button class="btn cancel-btn" onclick={handleCancel} data-testid="planetary-cancel-btn">Cancel</button>
			<button class="btn apply-btn" onclick={handleCreate} disabled={blocking || creating}
				data-testid="planetary-create-btn">Create</button>
		</div>
	</div>
{/if}

<style>
	.planetary-dialog {
		position: absolute;
		right: max(16px, env(safe-area-inset-right, 0px));
		top: 60px;
		width: 280px;
		background: var(--bg-tertiary, #2a2a3e);
		border: 1px solid var(--border-color, #3a3a4e);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		z-index: 100;
		font-size: 13px;
		color: var(--text-primary, #e0e0e0);
	}
	.planetary-dialog.mobile {
		position: fixed; right: 0; left: 0; bottom: 0; top: auto;
		width: 100%; max-height: 70vh; overflow-y: auto;
		border-radius: 12px 12px 0 0;
		padding-bottom: env(safe-area-inset-bottom, 0px);
	}
	.dialog-header {
		display: flex; justify-content: space-between; align-items: center;
		padding: 10px 12px; border-bottom: 1px solid var(--border-color, #3a3a4e);
	}
	.dialog-title { font-weight: 600; font-size: 14px; }
	.close-btn {
		background: none; border: none; color: var(--text-secondary, #999);
		font-size: 18px; cursor: pointer; padding: 0 4px;
	}
	.close-btn:hover { color: var(--text-primary, #e0e0e0); }
	.dialog-body { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
	.param-row { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
	.param-row label { font-size: 12px; color: var(--text-secondary, #999); white-space: nowrap; }
	.param-row input {
		width: 80px; background: var(--bg-primary, #1a1a2e);
		border: 1px solid var(--border-color, #3a3a4e); border-radius: 4px;
		color: var(--text-primary, #e0e0e0); padding: 4px 6px; font-size: 12px; text-align: right;
	}
	.param-row input[type="checkbox"] { width: auto; }
	.param-row input:focus { border-color: var(--accent, #0078d4); outline: none; }
	.derived-value { font-size: 12px; color: var(--text-secondary, #999); font-style: italic; }
	.divider { height: 1px; background: var(--border-color, #3a3a4e); margin: 2px 0; }
	.hints {
		font-size: 11px; padding: 6px 8px; border-radius: 4px;
		background: rgba(255, 196, 0, 0.1); color: var(--text-secondary, #ccc);
	}
	.hints.blocking { background: rgba(220, 64, 64, 0.15); color: #ff9090; }
	.hint { margin: 2px 0; }
	.hint.info { color: #8fd0ff; }
	.dialog-footer {
		display: flex; justify-content: flex-end; gap: 8px;
		padding: 8px 12px; border-top: 1px solid var(--border-color, #3a3a4e);
	}
	.btn { padding: 6px 16px; border-radius: 4px; font-size: 12px; cursor: pointer; border: 1px solid transparent; }
	.cancel-btn { background: transparent; color: var(--text-secondary, #999); border-color: var(--border-color, #3a3a4e); }
	.cancel-btn:hover { background: var(--bg-hover, #333); }
	.apply-btn { background: var(--accent, #0078d4); color: white; border: none; }
	.apply-btn:hover:not(:disabled) { background: #006abc; }
	.apply-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
