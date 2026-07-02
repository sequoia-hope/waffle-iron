<script>
	import {
		getExtrudeDialogState,
		hideExtrudeDialog,
		applyExtrude,
		removeExtrudeRegion,
		clearExtrudeRegions,
		setExtrudePreviewParams,
		changeExtrudeSketch,
		addExtrudeRegion,
		setProfilePickMode,
		getProfilePickMode,
		getDocumentDisplayUnit,
		getBodies,
		getExtrudeTargetPick,
		setExtrudeTargetPickActive,
		setExtrudeTargetIds,
		toggleExtrudeTargetId,
		clearExtrudeTargets
	} from '$lib/engine/store.svelte.js';
	import { showToast } from '$lib/ui/toast.svelte.js';
	import { log } from '$lib/engine/logger.js';
	import { displayToInternal, internalToDisplay, parseAndConvert, formatForInput, UNITS } from '$lib/units.js';

	let dialogState = $derived(getExtrudeDialogState());
	let displayUnit = $derived(getDocumentDisplayUnit());
	let unitLabel = $derived(UNITS[displayUnit]?.label ?? displayUnit);
	let depthInput = $state('10');
	// Optional-boolean combine mode: 'NewBody' | 'Add' | 'Cut' | 'Intersect'.
	// Default 'Add' with Auto targets = the share-a-face auto-merge behavior.
	let combine = $state('Add');
	// Target selection: 'Auto' (share-a-face, targets=null) or 'Choose' (explicit).
	let targetMode = $state('Auto');
	// Selected target ids + viewport-pick flag live in the store so a body can be
	// picked either from this list or by clicking it in the 3D viewport.
	let selectedTargetIds = $derived(getExtrudeTargetPick().ids);
	let targetPickActive = $derived(getExtrudeTargetPick().active);
	// Derived legacy cut flag for the ghost preview.
	let cut = $derived(combine === 'Cut');
	let bodies = $derived(getBodies());
	let depthMode = $state('Blind');
	let secondDir = $state('None');
	let secondDepthInput = $state('10');
	let flipDirection = $state(false);

	let showDepthInput = $derived(depthMode === 'Blind');
	let depthLabel = $derived(secondDir === 'Symmetric' ? 'Depth (each side)' : 'Depth');
	let showSecondDepthInput = $derived(secondDir === 'Blind');

	// Don't let viewport target-pick linger once we leave "Choose bodies" — else
	// body clicks would keep hijacking normal face selection.
	$effect(() => {
		if ((combine === 'NewBody' || targetMode !== 'Choose') && targetPickActive) {
			setExtrudeTargetPickActive(false);
		}
	});

	let regions = $derived(dialogState?.regions ?? []);
	let availableSketches = $derived(dialogState?.availableSketches ?? []);

	$effect(() => {
		if (dialogState) {
			const ep = dialogState.editParams;
			if (ep) {
				depthInput = formatForInput(ep.depth, displayUnit);
				// Prefer the explicit combine; else derive from legacy cut/merge.
				if (ep.combine?.type) combine = ep.combine.type;
				else if (ep.cut) combine = 'Cut';
				else if (ep.merge === false) combine = 'NewBody';
				else combine = 'Add';
				if (Array.isArray(ep.targets)) {
					targetMode = 'Choose';
					setExtrudeTargetIds(
						ep.targets
							.map((t) =>
								t?.anchor?.feature_id
									? `${t.anchor.feature_id}/${t.anchor.output_key?.type ?? 'Main'}`
									: null
							)
							.filter(Boolean)
					);
				} else {
					targetMode = 'Auto';
					clearExtrudeTargets();
				}
				depthMode = ep.depth_mode?.type ?? 'Blind';
				if (ep.symmetric) secondDir = 'Symmetric';
				else if (ep.second_direction) secondDir = ep.second_direction.type ?? 'None';
				else secondDir = 'None';
				secondDepthInput = ep.second_direction?.depth != null ? formatForInput(ep.second_direction.depth, displayUnit) : '10';
				flipDirection = ep.direction != null;
			} else {
				depthInput = '10';
				combine = 'Add';
				targetMode = 'Auto';
				clearExtrudeTargets();
				depthMode = 'Blind';
				secondDir = 'None';
				secondDepthInput = '10';
				flipDirection = false;
			}
		}
	});

	// Compute internal depth from display input for preview and apply
	let depth = $derived(parseAndConvert(depthInput, displayUnit));
	let secondDepth = $derived(parseAndConvert(secondDepthInput, displayUnit));

	// Drive ghost preview params whenever dialog state changes
	$effect(() => {
		if (!dialogState || depthMode !== 'Blind' || isNaN(depth)) {
			setExtrudePreviewParams(null);
			return;
		}
		const params = regions.map(r => {
			if (r.type === 'sketchProfile' || (!r.type && r.sketchId)) {
				return { type: 'sketchProfile', sketchId: r.sketchId, profileIndex: r.profileIndex ?? 0, region: r.region ?? null, depth, flipDirection, symmetric: secondDir === 'Symmetric', cut };
			}
			if (r.type === 'face') {
				return { type: 'face', geomRef: r.geomRef, depth, flipDirection, symmetric: secondDir === 'Symmetric', cut };
			}
			return null;
		}).filter(Boolean);
		setExtrudePreviewParams(params.length > 0 ? params : null);
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
		const firstRegion = regions[0];
		if (!firstRegion) return;

		if (regions.length > 1) {
			log('ui', `Multi-region extrude: using first region (${regions.length} selected)`);
		}

		if (firstRegion.type === 'face') {
			showToast('warning', 'Face-based extrude not yet supported by engine');
			return;
		}

		// Auto (share-a-face) ⇒ targets null; Choose ⇒ explicit GeomRefs (an empty
		// list forces a new body). New Body ignores targets entirely.
		let targets = null;
		if (combine !== 'NewBody' && targetMode === 'Choose') {
			const all = getBodies();
			targets = selectedTargetIds
				.map((id) => all.find((b) => b.bodyId === id))
				.filter(Boolean)
				.map(bodyToGeomRef);
		}

		const opts = {
			depthMode,
			secondDir,
			secondDepth,
			flipDirection,
			combine,
			targets
		};
		applyExtrude(depth, firstRegion.profileIndex ?? 0, cut, opts)
			.catch(err => log('error', `Extrude dialog apply failed: ${err}`));
	}

	/** Build a body GeomRef (Main output) for the combine target list. */
	function bodyToGeomRef(body) {
		return {
			kind: { type: 'Solid' },
			anchor: {
				type: 'FeatureOutput',
				feature_id: body.featureId,
				output_key: body.outputKey ?? { type: 'Main' }
			},
			selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
			policy: { type: 'BestEffort' }
		};
	}

	function toggleTarget(bodyId) {
		toggleExtrudeTargetId(bodyId);
	}

	function handleCancel() {
		clearExtrudeTargets();
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

	let regionPickActive = $derived(getProfilePickMode()?.target === 'extrude');

	function toggleRegionPick() {
		if (regionPickActive) {
			setProfilePickMode(null);
		} else {
			clearExtrudeRegions();
			setProfilePickMode({ target: 'extrude' });
		}
	}

	function regionLabel(region) {
		if (region.type === 'sketchProfile') {
			// A genuine sub-region (annulus, lens, …) has no whole-loop profile.
			if (region.region && region.region.profile_entity_ids == null) {
				return `${region.sketchName} / Region`;
			}
			return `${region.sketchName} / Profile ${region.profileIndex + 1}`;
		}
		if (region.type === 'face') return region.label || 'Face';
		// Legacy fallback
		return `${region.sketchName || '?'} / Profile ${(region.profileIndex ?? 0) + 1}`;
	}

	// Track dialog open/close to auto-enter pick mode only on open
	let prevDialogOpen = false;
	$effect(() => {
		const isOpen = !!dialogState;
		if (isOpen && !prevDialogOpen) {
			// Dialog just opened — auto-enter pick mode
			setProfilePickMode({ target: 'extrude' });
		} else if (!isOpen && prevDialogOpen) {
			// Dialog closed — deactivate pick mode
			setProfilePickMode(null);
		}
		prevDialogOpen = isOpen;
	});
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="extrude-panel" onkeydown={handleKeydown} data-testid="extrude-dialog">
		<div class="dialog-header">
			<span class="dialog-title">{dialogState.editingFeatureId ? 'Edit Extrude' : 'Extrude'}</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			{#if availableSketches.length > 1}
				<div class="field">
					<label for="extrude-sketch">Sketch</label>
					<select
						id="extrude-sketch"
						data-testid="extrude-sketch-select"
						value={dialogState.sketchId}
						onchange={(e) => changeExtrudeSketch(e.target.value)}
					>
						{#each availableSketches as sketch}
							<option value={sketch.id}>{sketch.name}</option>
						{/each}
					</select>
				</div>
			{/if}
			<div
				class="region-box"
				class:active={regionPickActive}
				role="button"
				tabindex="0"
				onclick={toggleRegionPick}
				data-testid="extrude-region-box"
			>
				<div class="region-box-header">
					<span class="region-header">Regions ({regions.length})</span>
					<span class="pick-hint">
						{regionPickActive ? 'Click sketch profiles or faces...' : 'Click to pick'}
					</span>
				</div>
				{#each regions as region, i}
					<div class="region-item" data-testid="extrude-region-{i}">
						<span class="region-label">{regionLabel(region)}</span>
						<button
							class="region-remove"
							onclick={(e) => { e.stopPropagation(); handleRemoveRegion(i); }}
						>&times;</button>
					</div>
				{/each}
				{#if regions.length === 0}
					<div class="region-empty">No regions — click sketch profiles or faces</div>
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
					<label for="extrude-depth">{depthLabel} ({unitLabel})</label>
					<input
						id="extrude-depth"
						data-testid="extrude-depth"
						type="text"
						inputmode="decimal"
						bind:value={depthInput}
						placeholder={unitLabel}
					/>
				</div>
			{/if}
			<div class="field">
				<label for="extrude-combine">Combine</label>
				<select
					id="extrude-combine"
					data-testid="extrude-combine"
					bind:value={combine}
				>
					<option value="NewBody">New Body</option>
					<option value="Add">Add</option>
					<option value="Cut">Cut</option>
					<option value="Intersect">Intersect</option>
				</select>
			</div>
			{#if combine !== 'NewBody'}
				<div class="field">
					<label for="extrude-target-mode">Targets</label>
					<select
						id="extrude-target-mode"
						data-testid="extrude-target-mode"
						bind:value={targetMode}
					>
						<option value="Auto">Auto (bodies touching sketch)</option>
						<option value="Choose">Choose bodies…</option>
					</select>
				</div>
				{#if targetMode === 'Choose'}
					<div class="field">
						<label for="extrude-target-pick">In viewport</label>
						<button
							id="extrude-target-pick"
							class="btn btn-flip"
							class:flipped={targetPickActive}
							data-testid="extrude-target-pick"
							onclick={() => setExtrudeTargetPickActive(!targetPickActive)}
						>
							{targetPickActive ? 'Picking… (click bodies)' : 'Pick in viewport'}
						</button>
					</div>
					<div class="target-list" data-testid="extrude-target-list">
						{#each bodies as body}
							<label class="target-item">
								<input
									type="checkbox"
									checked={selectedTargetIds.includes(body.bodyId)}
									onchange={() => toggleTarget(body.bodyId)}
								/>
								<span>{body.name}</span>
							</label>
						{/each}
						{#if bodies.length === 0}
							<div class="region-empty">No bodies yet</div>
						{/if}
					</div>
				{/if}
			{/if}
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
					<label for="extrude-second-depth">2nd Depth ({unitLabel})</label>
					<input
						id="extrude-second-depth"
						data-testid="extrude-second-depth"
						type="text"
						inputmode="decimal"
						bind:value={secondDepthInput}
						placeholder={unitLabel}
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
		right: max(12px, env(safe-area-inset-right, 0px));
		width: 240px;
		z-index: 50;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		pointer-events: auto;
	}

	@media (max-width: 768px) {
		.extrude-panel {
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

	.region-box {
		border: 2px solid var(--border-color, #444);
		border-radius: 4px;
		padding: 8px;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.region-box:hover { border-color: var(--accent, #0078d4); }
	.region-box.active {
		border-color: var(--accent, #0078d4);
		background: rgba(0, 120, 212, 0.1);
		animation: pulse-border 1.5s ease-in-out infinite;
	}

	@keyframes pulse-border {
		0%, 100% { border-color: var(--accent, #0078d4); }
		50% { border-color: rgba(0, 120, 212, 0.4); }
	}
	.region-box-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.pick-hint {
		font-size: 10px;
		color: var(--text-muted, #888);
		font-style: italic;
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

	.target-list {
		display: flex;
		flex-direction: column;
		gap: 2px;
		max-height: 120px;
		overflow-y: auto;
		border: 1px solid var(--border, #444);
		border-radius: 4px;
		padding: 4px 6px;
	}
	.target-item {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		cursor: pointer;
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
	.field input[type="text"],
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
		color: var(--text-on-accent);
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

	.btn-apply:hover:not(:disabled) {
		filter: brightness(1.1);
	}

	.btn-apply:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
