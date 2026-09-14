<script>
	/**
	 * Part mate connector dialog (`specs/part_mate_connectors.md`): a named
	 * frame on a face or an edge of the part — or at the part origin — with
	 * the adjustments an assembly connector has (anchor on a rotational face's
	 * axis, flip z, turn about z, offset along its own axes). Every instance
	 * of the part in an assembly then offers it for mates.
	 *
	 * The reference follows the viewport: open the dialog with a face or edge
	 * selected, or select one while it is open and press "use selection".
	 */
	import {
		getMateConnectorDialogState,
		hideMateConnectorDialog,
		applyMateConnector,
		getSelectedRefs,
		getPartConnectorFrames,
		getFeatureErrors,
		CONNECTOR_ANCHORS
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	const ANCHOR_LABELS = { middle: 'middle', positive_end: '+z end', negative_end: '−z end' };

	let dialogState = $derived(getMateConnectorDialogState());
	let name = $state('');
	let geomRef = $state(null);
	let anchor = $state('middle');
	let flipZ = $state(false);
	let rotationDeg = $state(0);
	let offsetMm = $state([0, 0, 0]);
	let busy = $state(false);

	// Fill the fields when the dialog opens — not on every state change: a
	// failed apply keeps the dialog open on the new feature, and the user's
	// choice must survive that.
	let wasOpen = false;
	$effect(() => {
		const s = dialogState;
		if (s && !wasOpen) {
			name = s.name;
			geomRef = s.geomRef;
			anchor = s.anchor;
			flipZ = s.flipZ;
			rotationDeg = s.rotationDeg;
			offsetMm = [...s.offsetMm];
		}
		wasOpen = !!s;
	});

	let selectedPick = $derived(
		getSelectedRefs().find((r) => r?.kind?.type === 'Face' || r?.kind?.type === 'Edge') ?? null
	);
	let pickIsNew = $derived(!!selectedPick && JSON.stringify(selectedPick) !== JSON.stringify(geomRef));
	let referenceLabel = $derived(
		geomRef ? `selected ${geomRef.kind?.type === 'Edge' ? 'edge' : 'face'}` : 'part origin (z up)'
	);
	let evaluated = $derived(
		dialogState?.editingFeatureId
			? getPartConnectorFrames().find((c) => c.feature_id === dialogState.editingFeatureId) ?? null
			: null
	);
	let error = $derived(
		dialogState?.editingFeatureId ? getFeatureErrors().get(dialogState.editingFeatureId) ?? null : null
	);

	function useSelection() {
		if (selectedPick) geomRef = JSON.parse(JSON.stringify(selectedPick));
	}

	async function handleApply() {
		if (busy) return;
		busy = true;
		try {
			await applyMateConnector({ name, geomRef, anchor, flipZ, rotationDeg, offsetMm });
		} catch (err) {
			log('error', `Mate connector dialog apply failed: ${err}`);
		} finally {
			busy = false;
		}
	}

	$effect(() => {
		if (!dialogState) return;
		function onKeyDown(e) {
			if (e.key === 'Escape') {
				e.preventDefault();
				e.stopPropagation();
				hideMateConnectorDialog();
			} else if (e.key === 'Enter' && !(e.target instanceof HTMLSelectElement)) {
				e.preventDefault();
				e.stopPropagation();
				handleApply();
			}
		}
		window.addEventListener('keydown', onKeyDown, { capture: true });
		return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
	});
</script>

{#if dialogState}
	<div class="mc-panel" data-testid="mate-connector-dialog">
		<div class="dialog-header">
			<span class="dialog-title">{dialogState.editingFeatureId ? 'Edit Mate Connector' : 'Mate Connector'}</span>
			<button class="close-btn" onclick={() => hideMateConnectorDialog()}>&times;</button>
		</div>
		<div class="dialog-body">
			<div class="field">
				<label for="mc-name">Name</label>
				<input id="mc-name" data-testid="mc-name" placeholder="Mate connector" bind:value={name} />
			</div>
			<div class="field">
				<span class="field-label">On</span>
				<div class="reference">
					<span data-testid="mc-reference">{referenceLabel}</span>
					{#if evaluated?.kind}<span class="meta" data-testid="mc-kind">· {evaluated.kind}</span>{/if}
				</div>
				<div class="buttons">
					<button class="btn btn-small" data-testid="mc-use-selection" disabled={!pickIsNew} title="Place the connector on the face or edge selected in the viewport" onclick={useSelection}>use selection</button>
					<button class="btn btn-small" data-testid="mc-clear-reference" disabled={!geomRef} title="Place the connector at the part origin" onclick={() => (geomRef = null)}>origin</button>
				</div>
			</div>
			{#if geomRef?.kind?.type === 'Face'}
				<div class="field">
					<label for="mc-anchor" title="On a cylindrical, conical or toroidal face: the middle of the face along its axis, or the end z points toward (+z) or away from (−z)">Along the axis</label>
					<select id="mc-anchor" data-testid="mc-anchor" bind:value={anchor}>
						{#each CONNECTOR_ANCHORS as a}<option value={a}>{ANCHOR_LABELS[a]}</option>{/each}
					</select>
				</div>
			{/if}
			<div class="row">
				<label class="check" title="Reverse the z axis (the triad's blue arrow)"><input type="checkbox" data-testid="mc-flip" bind:checked={flipZ} /> flip z</label>
				<label class="inline" title="Turn about z (°)">turn <input class="num" type="number" step="15" data-testid="mc-rotation" bind:value={rotationDeg} />°</label>
			</div>
			<div class="field">
				<span class="field-label">Offset along its own x, y, z (mm)</span>
				<div class="row">
					{#each ['x', 'y', 'z'] as axis, k}
						<input class="num" type="number" step="0.5" title="offset along {axis} (mm)" data-testid="mc-o{axis}" bind:value={offsetMm[k]} />
					{/each}
				</div>
			</div>
			{#if error}
				<div class="err" data-testid="mc-error">{error}</div>
			{/if}
		</div>
		<div class="dialog-footer">
			<button class="btn btn-cancel" data-testid="mc-cancel" onclick={() => hideMateConnectorDialog()}>Cancel</button>
			<button class="btn btn-apply" data-testid="mc-apply" disabled={busy} onclick={handleApply}>Apply</button>
		</div>
	</div>
{/if}

<style>
	.mc-panel {
		position: absolute;
		top: 12px;
		right: max(12px, env(safe-area-inset-right, 0px));
		width: 260px;
		z-index: 50;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		pointer-events: auto;
	}

	@media (max-width: 768px) {
		.mc-panel {
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
		gap: 10px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.field label,
	.field-label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
	}

	.reference {
		font-size: 12px;
		color: var(--text-primary, #eee);
	}

	.meta {
		color: var(--text-muted, #888);
	}

	.row,
	.buttons {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		color: var(--text-primary, #eee);
	}

	.check,
	.inline {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	input:not([type='checkbox']),
	select {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 6px;
		border-radius: 3px;
		font-size: 12px;
	}

	.num {
		width: 56px;
	}

	input:focus,
	select:focus {
		outline: none;
		border-color: var(--accent, #0078d4);
	}

	.err {
		font-size: 12px;
		color: var(--error, #f48771);
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

	.btn-small {
		padding: 3px 8px;
		background: transparent;
		color: var(--text-secondary, #aaa);
		border-color: var(--border-color, #444);
	}

	.btn-small:disabled {
		opacity: 0.5;
		cursor: default;
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

	.btn-apply:hover:not(:disabled) {
		filter: brightness(1.1);
	}

	.btn-apply:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
