<script>
	import {
		getGearDialogState,
		hideGearDialog,
		createGear,
		updateGear,
		getMobileLayout,
		getDocumentDisplayUnit,
		getBridge
	} from '$lib/engine/store.svelte.js';
	import { setPreview } from '$lib/sketch/sketchToolState.svelte.js';
	import { log } from '$lib/engine/logger.js';
	import { internalToDisplay, displayToInternal, parseAndConvert, formatForInput, UNITS } from '$lib/units.js';
	import { DEFAULT_GEAR_MODULE_DISPLAY, DEFAULT_GEAR_TOOTH_COUNT, DEFAULT_GEAR_PRESSURE_ANGLE } from '$lib/config.js';

	let dialogState = $derived(getGearDialogState());
	let isMobile = $derived(getMobileLayout());
	let displayUnit = $derived(getDocumentDisplayUnit());
	let unitLabel = $derived(UNITS[displayUnit]?.label ?? displayUnit);

	let toothCount = $state(DEFAULT_GEAR_TOOTH_COUNT);
	let moduleInput = $state(DEFAULT_GEAR_MODULE_DISPLAY.mm);
	let pressureAngle = $state(DEFAULT_GEAR_PRESSURE_ANGLE);
	let backlash = $state(0);

	// Editing state
	let editingGearId = $state(null);

	/** Compute internal module from display input */
	let module_ = $derived(parseAndConvert(moduleInput, displayUnit) || displayToInternal(1, 'mm'));

	/** Default gear module display value based on unit system */
	function defaultModuleDisplay() {
		return DEFAULT_GEAR_MODULE_DISPLAY[displayUnit] ?? DEFAULT_GEAR_MODULE_DISPLAY.mm;
	}

	$effect(() => {
		if (dialogState) {
			if (dialogState.editGearId != null && dialogState.params) {
				// Edit mode: restore existing params
				editingGearId = dialogState.editGearId;
				toothCount = dialogState.params.toothCount ?? DEFAULT_GEAR_TOOTH_COUNT;
				moduleInput = formatForInput(dialogState.params.module ?? displayToInternal(1, 'mm'), displayUnit);
				pressureAngle = dialogState.params.pressureAngle ?? DEFAULT_GEAR_PRESSURE_ANGLE;
				backlash = dialogState.params.backlash ?? 0;
			} else {
				// Create mode
				editingGearId = null;
				toothCount = DEFAULT_GEAR_TOOTH_COUNT;
				moduleInput = dialogState.pitchDiameter
					? formatForInput(dialogState.pitchDiameter / DEFAULT_GEAR_TOOTH_COUNT, displayUnit)
					: defaultModuleDisplay();
				pressureAngle = DEFAULT_GEAR_PRESSURE_ANGLE;
				backlash = 0;
			}
		}
	});

	// Live preview via WASM — latest-wins pattern for rapid slider changes
	let previewGeneration = 0;
	$effect(() => {
		if (!dialogState) {
			setPreview(null);
			return;
		}
		const N = Math.max(4, Math.round(toothCount));
		const m = Math.max(0.0001, module_);
		const params = {
			toothCount: N,
			module: m,
			pressureAngleDeg: pressureAngle,
			backlash,
			centerX: dialogState.centerX ?? 0,
			centerY: dialogState.centerY ?? 0,
			rotationOffset: dialogState.rotationOffset ?? 0
		};
		const gen = ++previewGeneration;
		const bridge = getBridge();
		if (!bridge) return;
		bridge.send({ type: 'GenerateGearPreview', params }).then(response => {
			if (gen === previewGeneration) {
				setPreview({ type: 'gear-preview', data: { polyline: response.polyline } });
			}
		}).catch(() => { /* stale or bridge error — ignore */ });
	});

	let pitchDiameter = $derived(toothCount * module_);

	$effect(() => {
		if (!dialogState) return;
		function onKeyDown(e) {
			if (e.key === 'Enter') {
				e.preventDefault();
				e.stopPropagation();
				handleApply();
			} else if (e.key === 'Escape') {
				e.preventDefault();
				e.stopPropagation();
				handleCancel();
			}
		}
		window.addEventListener('keydown', onKeyDown, { capture: true });
		return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
	});

	async function handleApply() {
		const N = Math.max(4, Math.round(toothCount));
		const m = Math.max(0.0001, module_);
		const params = {
			toothCount: N,
			module: m,
			pressureAngleDeg: pressureAngle,
			backlash,
			centerX: dialogState.centerX ?? 0,
			centerY: dialogState.centerY ?? 0,
			rotationOffset: dialogState.rotationOffset ?? 0
		};

		if (editingGearId != null) {
			await updateGear(editingGearId, params);
			log('sketch', `Gear updated: ${N} teeth, module ${m}`);
		} else {
			await createGear(params);
			log('sketch', `Gear created: ${N} teeth, module ${m}`);
		}

		setPreview(null);
		hideGearDialog();
	}

	function handleCancel() {
		setPreview(null);
		hideGearDialog();
	}
</script>

{#if dialogState}
	<div class="gear-dialog" class:mobile={isMobile} data-testid="gear-dialog">
		<div class="dialog-header">
			<span class="dialog-title">{editingGearId != null ? 'Edit' : 'Create'} Gear</span>
			<button class="close-btn" onclick={handleCancel} data-testid="gear-dialog-close">&times;</button>
		</div>

		<div class="dialog-body">
			<div class="param-row">
				<label for="gear-teeth">Teeth (N)</label>
				<input
					id="gear-teeth"
					type="number"
					min="4"
					step="1"
					bind:value={toothCount}
					data-testid="gear-teeth-input"
				/>
			</div>

			<div class="param-row">
				<label for="gear-module">Module ({unitLabel})</label>
				<input
					id="gear-module"
					type="text"
					inputmode="decimal"
					bind:value={moduleInput}
					disabled={dialogState.diameterLocked}
					placeholder={unitLabel}
					data-testid="gear-module-input"
				/>
			</div>

			<div class="param-row">
				<label>Pitch Diameter</label>
				<span class="derived-value" data-testid="gear-pitch-diameter">
					{internalToDisplay(pitchDiameter, displayUnit).toFixed(2)} {unitLabel}
				</span>
			</div>

			<div class="param-row">
				<label for="gear-pressure">Pressure Angle</label>
				<input
					id="gear-pressure"
					type="number"
					min="14.5"
					max="30"
					step="0.5"
					bind:value={pressureAngle}
					data-testid="gear-pressure-input"
				/>
			</div>

			<div class="param-row">
				<label for="gear-backlash">Backlash</label>
				<input
					id="gear-backlash"
					type="number"
					min="0"
					step="0.01"
					bind:value={backlash}
					data-testid="gear-backlash-input"
				/>
			</div>
		</div>

		<div class="dialog-footer">
			<button class="btn cancel-btn" onclick={handleCancel} data-testid="gear-cancel-btn">Cancel</button>
			<button class="btn apply-btn" onclick={handleApply} data-testid="gear-apply-btn">Apply</button>
		</div>
	</div>
{/if}

<style>
	.gear-dialog {
		position: absolute;
		right: max(16px, env(safe-area-inset-right, 0px));
		top: 60px;
		width: 260px;
		background: var(--bg-tertiary, #2a2a3e);
		border: 1px solid var(--border-color, #3a3a4e);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		z-index: 100;
		font-size: 13px;
		color: var(--text-primary, #e0e0e0);
	}

	.gear-dialog.mobile {
		position: fixed;
		right: 0;
		left: 0;
		bottom: 0;
		top: auto;
		width: 100%;
		max-height: 60vh;
		border-radius: 12px 12px 0 0;
		padding-bottom: env(safe-area-inset-bottom, 0px);
	}

	.dialog-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-color, #3a3a4e);
	}

	.dialog-title {
		font-weight: 600;
		font-size: 14px;
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-secondary, #999);
		font-size: 18px;
		cursor: pointer;
		padding: 0 4px;
	}

	.close-btn:hover {
		color: var(--text-primary, #e0e0e0);
	}

	.dialog-body {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.param-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 8px;
	}

	.param-row label {
		font-size: 12px;
		color: var(--text-secondary, #999);
		white-space: nowrap;
	}

	.param-row input {
		width: 80px;
		background: var(--bg-primary, #1a1a2e);
		border: 1px solid var(--border-color, #3a3a4e);
		border-radius: 4px;
		color: var(--text-primary, #e0e0e0);
		padding: 4px 6px;
		font-size: 12px;
		text-align: right;
	}

	.param-row input:focus {
		border-color: var(--accent, #0078d4);
		outline: none;
	}

	.param-row input:disabled {
		opacity: 0.5;
	}

	.derived-value {
		font-size: 12px;
		color: var(--text-secondary, #999);
		font-style: italic;
	}

	.dialog-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 8px 12px;
		border-top: 1px solid var(--border-color, #3a3a4e);
	}

	.btn {
		padding: 6px 16px;
		border-radius: 4px;
		font-size: 12px;
		cursor: pointer;
		border: 1px solid transparent;
	}

	.cancel-btn {
		background: transparent;
		color: var(--text-secondary, #999);
		border-color: var(--border-color, #3a3a4e);
	}

	.cancel-btn:hover {
		background: var(--bg-hover, #333);
	}

	.apply-btn {
		background: var(--accent, #0078d4);
		color: white;
		border: none;
	}

	.apply-btn:hover {
		background: #006abc;
	}
</style>
