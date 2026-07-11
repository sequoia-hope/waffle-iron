<script>
	import {
		getImportDialogState,
		hideImportDialog,
		applyImportPlacement,
		getFeatureTree,
		getDocumentDisplayUnit
	} from '$lib/engine/store.svelte.js';
	import { formatForInput, parseAndConvert } from '$lib/units.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getImportDialogState());
	let unit = $derived(getDocumentDisplayUnit());

	// Translation fields are edited in the display unit; rotation in degrees.
	let tx = $state('0');
	let ty = $state('0');
	let tz = $state('0');
	let rx = $state(0);
	let ry = $state(0);
	let rz = $state(0);
	let scale = $state(1.0);
	let fileName = $state('');

	// Seed the fields from the feature's current params when opening.
	$effect(() => {
		if (!dialogState) return;
		const feature = getFeatureTree()?.features?.find(f => f.id === dialogState.featureId);
		const params = feature?.operation?.params;
		if (!params) return;
		fileName = params.file_name ?? '';
		const t = params.translation_m ?? [0, 0, 0];
		tx = formatForInput(t[0], unit);
		ty = formatForInput(t[1], unit);
		tz = formatForInput(t[2], unit);
		const r = params.rotation_deg ?? [0, 0, 0];
		rx = r[0];
		ry = r[1];
		rz = r[2];
		scale = params.scale ?? 1.0;
	});

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

	function handleApply() {
		const translation_m = [
			parseAndConvert(tx, unit),
			parseAndConvert(ty, unit),
			parseAndConvert(tz, unit),
		];
		if (translation_m.some(v => v == null || Number.isNaN(v))) {
			log('error', 'Import placement: invalid translation value');
			return;
		}
		applyImportPlacement(dialogState.featureId, {
			translation_m,
			rotation_deg: [Number(rx) || 0, Number(ry) || 0, Number(rz) || 0],
			scale: Number(scale) || 1.0,
		}).catch(err => log('error', `Import placement apply failed: ${err}`));
	}

	function handleCancel() {
		hideImportDialog();
	}
</script>

{#if dialogState}
	<div class="import-panel" data-testid="import-step-dialog">
		<div class="dialog-header">
			<span class="dialog-title">Position {fileName}</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			<span class="group-label">Offset ({unit})</span>
			<div class="field-row">
				<label for="import-tx">X</label>
				<input id="import-tx" data-testid="import-tx" type="text" bind:value={tx} />
				<label for="import-ty">Y</label>
				<input id="import-ty" data-testid="import-ty" type="text" bind:value={ty} />
				<label for="import-tz">Z</label>
				<input id="import-tz" data-testid="import-tz" type="text" bind:value={tz} />
			</div>
			<span class="group-label">Rotation (deg)</span>
			<div class="field-row">
				<label for="import-rx">X</label>
				<input id="import-rx" data-testid="import-rx" type="number" step="1" bind:value={rx} />
				<label for="import-ry">Y</label>
				<input id="import-ry" data-testid="import-ry" type="number" step="1" bind:value={ry} />
				<label for="import-rz">Z</label>
				<input id="import-rz" data-testid="import-rz" type="number" step="1" bind:value={rz} />
			</div>
			<div class="field-row scale-row">
				<label for="import-scale">Scale</label>
				<input id="import-scale" data-testid="import-scale" type="number" step="0.001" min="0.000001" bind:value={scale} />
			</div>
		</div>
		<div class="dialog-footer">
			<button class="btn btn-cancel" data-testid="import-cancel" onclick={handleCancel}>Cancel</button>
			<button class="btn btn-apply" data-testid="import-apply" onclick={handleApply}>Apply</button>
		</div>
	</div>
{/if}

<style>
	.import-panel {
		position: absolute;
		top: 12px;
		right: max(12px, env(safe-area-inset-right, 0px));
		width: 280px;
		z-index: 50;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		pointer-events: auto;
	}

	@media (max-width: 768px) {
		.import-panel {
			position: fixed;
			top: auto;
			right: 0;
			bottom: 0;
			left: 0;
			width: 100%;
			max-height: 60vh;
			border-radius: 12px 12px 0 0;
			overflow-y: auto;
			z-index: 150;
			padding-bottom: env(safe-area-inset-bottom, 0px);
		}
	}

	.dialog-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-color, #444);
	}

	.dialog-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary, #eee);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 18px;
		cursor: pointer;
		padding: 0 2px;
		line-height: 1;
	}

	.close-btn:hover {
		color: var(--text-primary, #eee);
	}

	.dialog-body {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.group-label {
		font-size: 11px;
		color: var(--text-muted, #888);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.field-row {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.field-row label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
	}

	.field-row input {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 6px;
		border-radius: 3px;
		font-size: 12px;
		width: 100%;
		min-width: 0;
	}

	.scale-row input {
		width: 120px;
		flex: none;
	}

	.field-row input:focus {
		outline: none;
		border-color: var(--accent, #0078d4);
	}

	.dialog-footer {
		display: flex;
		justify-content: flex-end;
		gap: 6px;
		padding: 8px 12px;
		border-top: 1px solid var(--border-color, #444);
	}

	.btn {
		padding: 5px 14px;
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
		border: 1px solid transparent;
	}

	.btn-cancel {
		background: transparent;
		color: var(--text-secondary, #aaa);
		border-color: var(--border-color, #444);
	}

	.btn-cancel:hover {
		background: var(--bg-hover, #333);
	}

	.btn-apply {
		background: var(--accent, #0078d4);
		color: var(--text-on-accent);
		border-color: var(--accent, #0078d4);
	}

	.btn-apply:hover {
		filter: brightness(1.1);
	}
</style>
