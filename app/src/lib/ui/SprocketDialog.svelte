<script>
	import {
		getSprocketDialogState,
		hideSprocketDialog,
		createSprocket,
		updateGear,
		getMobileLayout,
		getDocumentDisplayUnit,
		getBridge
	} from '$lib/engine/store.svelte.js';
	import { setPreview } from '$lib/sketch/sketchToolState.svelte.js';
	import { log } from '$lib/engine/logger.js';
	import { internalToDisplay, parseAndConvert, formatForInput, UNITS } from '$lib/units.js';
	import {
		DEFAULT_SPROCKET_TOOTH_COUNT,
		SPROCKET_CHAIN_PRESETS,
		DEFAULT_SPROCKET_CHAIN_PRESET
	} from '$lib/config.js';

	// Mirrors GearDialog.svelte: the sprocket is one compact `Sprocket` sketch
	// entity (spec `specs/custom_features_and_modeling_roadmap.md` §B3), created
	// through `createSprocket` / edited through the shared gear registry.

	let dialogState = $derived(getSprocketDialogState());
	let isMobile = $derived(getMobileLayout());
	let displayUnit = $derived(getDocumentDisplayUnit());
	let unitLabel = $derived(UNITS[displayUnit]?.label ?? displayUnit);

	let toothCount = $state(DEFAULT_SPROCKET_TOOTH_COUNT);
	let chainPreset = $state(DEFAULT_SPROCKET_CHAIN_PRESET);
	let pitchInput = $state('');
	let rollerInput = $state('');

	// Editing state
	let editingGearId = $state(null);

	/** Internal (metre) values parsed from the display inputs; NaN when blank/invalid. */
	let pitch = $derived(parseAndConvert(pitchInput, displayUnit));
	let rollerDiameter = $derived(parseAndConvert(rollerInput, displayUnit));

	/** The engine's refusal for the current parameters (typed, names the value), or null. */
	let previewError = $state(null);

	function presetById(id) {
		return SPROCKET_CHAIN_PRESETS.find((p) => p.id === id) ?? null;
	}

	/** Seed pitch/roller from a preset (display units). */
	function applyPreset(id) {
		const p = presetById(id);
		if (!p) return;
		pitchInput = formatForInput(p.pitch, displayUnit);
		rollerInput = formatForInput(p.roller, displayUnit);
	}

	/** The preset whose pitch and roller both match the inputs, else 'custom'. */
	function matchingPreset(p, d1) {
		const close = (a, b) => Math.abs(a - b) <= 1e-9;
		return SPROCKET_CHAIN_PRESETS.find((c) => close(c.pitch, p) && close(c.roller, d1))?.id ?? 'custom';
	}

	$effect(() => {
		if (dialogState) {
			previewError = null;
			if (dialogState.editGearId != null && dialogState.params) {
				// Edit mode: restore existing params
				editingGearId = dialogState.editGearId;
				const p = dialogState.params;
				toothCount = p.toothCount ?? DEFAULT_SPROCKET_TOOTH_COUNT;
				pitchInput = formatForInput(p.pitch, displayUnit);
				rollerInput = formatForInput(p.rollerDiameter, displayUnit);
				chainPreset = matchingPreset(p.pitch, p.rollerDiameter);
			} else {
				// Create mode
				editingGearId = null;
				toothCount = DEFAULT_SPROCKET_TOOTH_COUNT;
				chainPreset = DEFAULT_SPROCKET_CHAIN_PRESET;
				applyPreset(DEFAULT_SPROCKET_CHAIN_PRESET);
			}
		}
	});

	function onPresetChange(e) {
		const id = e.target.value;
		chainPreset = id;
		if (id !== 'custom') applyPreset(id);
	}

	/** Editing either dimension by hand moves the preset to "Custom" unless it still matches one. */
	function onDimensionInput() {
		if (Number.isFinite(pitch) && Number.isFinite(rollerDiameter)) {
			chainPreset = matchingPreset(pitch, rollerDiameter);
		}
	}

	/** The params as the engine takes them (camelCase `SprocketParams`), or null if a field is blank. */
	function currentParams() {
		const N = Math.round(toothCount);
		if (!Number.isFinite(N) || !Number.isFinite(pitch) || !Number.isFinite(rollerDiameter)) return null;
		return {
			toothCount: N,
			pitch,
			rollerDiameter,
			centerX: dialogState?.centerX ?? 0,
			centerY: dialogState?.centerY ?? 0,
			rotationOffset: dialogState?.rotationOffset ?? 0
		};
	}

	// Live preview via WASM — latest-wins pattern for rapid changes. The engine
	// is the validator: a refusal (too few teeth, seat smaller than the roller,
	// flanks that cross) is shown in the dialog and disables Apply.
	let previewGeneration = 0;
	$effect(() => {
		if (!dialogState) {
			setPreview(null);
			return;
		}
		const params = currentParams();
		const gen = ++previewGeneration;
		if (!params) {
			previewError = 'Enter a pitch and a roller diameter';
			setPreview(null);
			return;
		}
		const bridge = getBridge();
		if (!bridge) return;
		bridge.send({ type: 'GenerateSprocketPreview', params }).then(response => {
			if (gen === previewGeneration) {
				previewError = null;
				setPreview({ type: 'gear-preview', data: { polyline: response.polyline } });
			}
		}).catch((err) => {
			if (gen === previewGeneration) {
				previewError = err?.message ? String(err.message) : String(err);
				setPreview(null);
			}
		});
	});

	/** ISO 606 pitch diameter d = p / sin(π/z); the tip diameter's mid-range value. */
	let pitchDiameter = $derived.by(() => {
		const N = Math.round(toothCount);
		if (!(N >= 3) || !Number.isFinite(pitch)) return NaN;
		return pitch / Math.sin(Math.PI / N);
	});

	let canApply = $derived(previewError == null && currentParams() != null);

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
		if (!canApply) return;
		const params = currentParams();
		try {
			if (editingGearId != null) {
				await updateGear(editingGearId, params);
				log('sketch', `Sprocket updated: ${params.toothCount} teeth, pitch ${params.pitch}`);
			} else {
				await createSprocket(params);
				log('sketch', `Sprocket created: ${params.toothCount} teeth, pitch ${params.pitch}`);
			}
		} catch (err) {
			// The engine refused the parameters (nothing was added) — keep the
			// dialog open and say why.
			previewError = err?.message ? String(err.message) : String(err);
			return;
		}

		setPreview(null);
		hideSprocketDialog();
	}

	function handleCancel() {
		setPreview(null);
		hideSprocketDialog();
	}
</script>

{#if dialogState}
	<div class="sprocket-dialog" class:mobile={isMobile} data-testid="sprocket-dialog">
		<div class="dialog-header">
			<span class="dialog-title">{editingGearId != null ? 'Edit' : 'Create'} Sprocket</span>
			<button class="close-btn" onclick={handleCancel} data-testid="sprocket-dialog-close">&times;</button>
		</div>

		<div class="dialog-body">
			<div class="param-row">
				<label for="sprocket-teeth">Teeth (z)</label>
				<input
					id="sprocket-teeth"
					type="number"
					min="5"
					step="1"
					bind:value={toothCount}
					data-testid="sprocket-teeth-input"
				/>
			</div>

			<div class="param-row">
				<label for="sprocket-chain">Chain</label>
				<select
					id="sprocket-chain"
					class="chain-select"
					value={chainPreset}
					onchange={onPresetChange}
					data-testid="sprocket-chain-select"
				>
					{#each SPROCKET_CHAIN_PRESETS as c}
						<option value={c.id}>{c.label}</option>
					{/each}
					<option value="custom">Custom</option>
				</select>
			</div>

			<div class="param-row">
				<label for="sprocket-pitch">Pitch ({unitLabel})</label>
				<input
					id="sprocket-pitch"
					type="text"
					inputmode="decimal"
					bind:value={pitchInput}
					oninput={onDimensionInput}
					placeholder={unitLabel}
					data-testid="sprocket-pitch-input"
				/>
			</div>

			<div class="param-row">
				<label for="sprocket-roller">Roller Ø ({unitLabel})</label>
				<input
					id="sprocket-roller"
					type="text"
					inputmode="decimal"
					bind:value={rollerInput}
					oninput={onDimensionInput}
					placeholder={unitLabel}
					data-testid="sprocket-roller-input"
				/>
			</div>

			<div class="param-row">
				<label>Pitch Diameter</label>
				<span class="derived-value" data-testid="sprocket-pitch-diameter">
					{Number.isFinite(pitchDiameter) ? `${internalToDisplay(pitchDiameter, displayUnit).toFixed(2)} ${unitLabel}` : '—'}
				</span>
			</div>

			{#if previewError}
				<div class="error-row" data-testid="sprocket-error">{previewError}</div>
			{/if}
		</div>

		<div class="dialog-footer">
			<button class="btn cancel-btn" onclick={handleCancel} data-testid="sprocket-cancel-btn">Cancel</button>
			<button class="btn apply-btn" onclick={handleApply} disabled={!canApply} data-testid="sprocket-apply-btn">Apply</button>
		</div>
	</div>
{/if}

<style>
	.sprocket-dialog {
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

	.sprocket-dialog.mobile {
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

	.param-row input,
	.param-row select {
		width: 80px;
		background: var(--bg-primary, #1a1a2e);
		border: 1px solid var(--border-color, #3a3a4e);
		border-radius: 4px;
		color: var(--text-primary, #e0e0e0);
		padding: 4px 6px;
		font-size: 12px;
		text-align: right;
	}

	.param-row select.chain-select {
		width: 150px;
		text-align: left;
	}

	.param-row input:focus,
	.param-row select:focus {
		border-color: var(--accent, #0078d4);
		outline: none;
	}

	.derived-value {
		font-size: 12px;
		color: var(--text-secondary, #999);
		font-style: italic;
	}

	.error-row {
		font-size: 11px;
		color: var(--error, #e06c75);
		line-height: 1.3;
		word-break: break-word;
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
		color: var(--text-on-accent);
		border: none;
	}

	.apply-btn:hover:not(:disabled) {
		background: var(--accent-hover);
	}

	.apply-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
