<script>
	import {
		getExtrudeDialogState,
		hideExtrudeDialog,
		applyExtrude,
		removeExtrudeRegion
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getExtrudeDialogState());
	let depth = $state(10);
	let cut = $state(false);
	let depthMode = $state('Blind');
	let secondDir = $state('None');
	let secondDepth = $state(10);
	let flipDirection = $state(false);

	let showDepthInput = $derived(depthMode === 'Blind');
	let depthLabel = $derived(secondDir === 'Symmetric' ? 'Depth (each side)' : 'Depth');
	let showSecondDepthInput = $derived(secondDir === 'Blind');

	let regions = $derived(dialogState?.regions ?? []);

	$effect(() => {
		if (dialogState) {
			depth = 10;
			cut = false;
			depthMode = 'Blind';
			secondDir = 'None';
			secondDepth = 10;
			flipDirection = false;
		}
	});

	// Listen for keydown at window level so Escape works even without focus
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
		const opts = {
			depthMode,
			secondDir,
			secondDepth,
			flipDirection
		};
		// regions[0] is read inside applyExtrude; profileIndex param is legacy fallback
		applyExtrude(depth, regions[0]?.profileIndex ?? 0, cut, opts)
			.catch(err => log('error', `Extrude dialog apply failed: ${err}`));
	}

	function handleCancel() {
		hideExtrudeDialog();
	}

	function handleKeydown(e) {
		if (e.key === 'Enter') {
			e.preventDefault();
			handleApply();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			handleCancel();
		}
	}

	function handleRemoveRegion(index) {
		removeExtrudeRegion(index);
	}
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="extrude-panel" onkeydown={handleKeydown} data-testid="extrude-dialog">
		<div class="dialog-header">
			<span class="dialog-title">Extrude</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			<div class="region-list" data-testid="extrude-regions">
				<div class="region-header">
					<span>Regions ({regions.length})</span>
				</div>
				{#each regions as region, i}
					<div class="region-item" data-testid="extrude-region-{i}">
						<span class="region-label">{region.sketchName} / Profile {region.profileIndex + 1}</span>
						<button class="region-remove" onclick={() => handleRemoveRegion(i)}>&times;</button>
					</div>
				{/each}
				{#if regions.length === 0}
					<div class="region-empty">No regions selected</div>
				{/if}
			</div>
			<div class="field">
				<label for="extrude-depth-mode">Mode</label>
				<select
					id="extrude-depth-mode"
					data-testid="extrude-depth-mode"
					bind:value={depthMode}
				>
					<option value="Blind">Blind</option>
					<option value="ThroughAll">Through All</option>
				</select>
			</div>
			{#if showDepthInput}
				<div class="field">
					<label for="extrude-depth">{depthLabel}</label>
					<input
						id="extrude-depth"
						data-testid="extrude-depth"
						type="number"
						bind:value={depth}
						step="1"
						min="0.1"
					/>
				</div>
			{/if}
			<div class="field">
				<label for="extrude-cut">Cut</label>
				<input
					id="extrude-cut"
					data-testid="extrude-cut"
					type="checkbox"
					bind:checked={cut}
				/>
			</div>
			<div class="field">
				<label for="extrude-second-dir">2nd Direction</label>
				<select
					id="extrude-second-dir"
					data-testid="extrude-second-dir"
					bind:value={secondDir}
				>
					<option value="None">None</option>
					<option value="Symmetric">Symmetric</option>
					<option value="Blind">Two Depths</option>
					<option value="ThroughAll">Through All</option>
				</select>
			</div>
			{#if showSecondDepthInput}
				<div class="field">
					<label for="extrude-second-depth">2nd Depth</label>
					<input
						id="extrude-second-depth"
						data-testid="extrude-second-depth"
						type="number"
						bind:value={secondDepth}
						step="1"
						min="0.1"
					/>
				</div>
			{/if}
			<div class="field">
				<label for="extrude-flip-dir">Direction</label>
				<button
					id="extrude-flip-dir"
					class="btn btn-flip"
					class:flipped={flipDirection}
					data-testid="extrude-flip-direction"
					onclick={() => { flipDirection = !flipDirection; }}
				>
					{flipDirection ? 'Flipped' : 'Normal'}
				</button>
			</div>
		</div>
		<div class="dialog-footer">
			<button class="btn btn-cancel" data-testid="extrude-cancel" onclick={handleCancel}>Cancel</button>
			<button class="btn btn-apply" data-testid="extrude-apply" onclick={handleApply}>Apply</button>
		</div>
	</div>
{/if}

<style>
	.extrude-panel {
		position: absolute;
		top: 12px;
		right: 12px;
		width: 240px;
		z-index: 50;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		pointer-events: auto;
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

	.region-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.region-header {
		font-size: 11px;
		color: var(--text-secondary, #aaa);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
	}

	.region-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 4px 8px;
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		border-radius: 3px;
		font-size: 12px;
		color: var(--text-primary, #eee);
	}

	.region-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.region-remove {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 14px;
		cursor: pointer;
		padding: 0 2px;
		line-height: 1;
		flex-shrink: 0;
	}

	.region-remove:hover {
		color: var(--error, #f44);
	}

	.region-empty {
		font-size: 11px;
		color: var(--text-muted, #888);
		font-style: italic;
		padding: 4px 0;
	}

	.field {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.field label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		min-width: 50px;
	}

	.field input[type="number"],
	.field select {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 8px;
		border-radius: 3px;
		font-size: 12px;
		width: 120px;
	}

	.field input[type="checkbox"] {
		width: auto;
		accent-color: var(--accent, #0078d4);
	}

	.field input:focus,
	.field select:focus {
		outline: none;
		border-color: var(--accent, #0078d4);
	}

	.btn-flip {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 12px;
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
		min-width: 80px;
	}

	.btn-flip:hover {
		border-color: var(--accent, #0078d4);
	}

	.btn-flip.flipped {
		background: var(--accent, #0078d4);
		border-color: var(--accent, #0078d4);
		color: #fff;
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
		color: #fff;
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
