<script>
	import {
		getRevolveDialogState,
		hideRevolveDialog,
		applyRevolve,
		setRevolvePreviewParams,
		setProfilePickMode,
		getProfilePickMode,
		setAxisPickMode,
		getAxisPickMode,
		getBodies,
		getExtrudeTargetPick,
		setExtrudeTargetPickActive,
		setExtrudeTargetIds,
		toggleExtrudeTargetId,
		clearExtrudeTargets
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getRevolveDialogState());
	let angle = $state(360);
	// Optional-boolean combine (mirrors the extrude dialog).
	let combine = $state('Add');
	let targetMode = $state('Auto');
	let selectedTargetIds = $derived(getExtrudeTargetPick().ids);
	let targetPickActive = $derived(getExtrudeTargetPick().active);
	let bodies = $derived(getBodies());

	let selectedAxis = $derived(dialogState?.selectedAxis);
	let selectedProfile = $derived(dialogState?.selectedProfile);

	let profilePickActive = $derived(getProfilePickMode()?.target === 'revolve');
	let axisPickActive = $derived(getAxisPickMode());

	// Track dialog open/close to auto-enter pick modes only on open
	let prevDialogOpen = false;
	$effect(() => {
		const isOpen = !!dialogState;
		if (isOpen && !prevDialogOpen) {
			// Dialog just opened
			const ep = dialogState.editParams;
			if (ep) {
				angle = ep.angle ?? 360;
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
			} else {
				angle = 360;
				combine = 'Add';
				targetMode = 'Auto';
				clearExtrudeTargets();
			}
			setAxisPickMode(true);
		} else if (!isOpen && prevDialogOpen) {
			// Dialog closed
			setProfilePickMode(null);
			setAxisPickMode(false);
		}
		prevDialogOpen = isOpen;
	});

	// Send preview params whenever axis/angle/profile changes
	$effect(() => {
		if (dialogState && selectedAxis && selectedProfile) {
			setRevolvePreviewParams({
				sketchId: selectedProfile.sketchId ?? dialogState.sketchId,
				profileIndex: selectedProfile.profileIndex ?? 0,
				angle,
				axisOrigin: [...selectedAxis.origin],
				axisDir: [...selectedAxis.direction]
			});
		} else {
			setRevolvePreviewParams(null);
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

	let hasAxis = $derived(selectedAxis != null);

	// Deactivate viewport target-pick when leaving "Choose bodies".
	$effect(() => {
		if ((combine === 'NewBody' || targetMode !== 'Choose') && targetPickActive) {
			setExtrudeTargetPickActive(false);
		}
	});

	function handleApply() {
		if (!hasAxis || !selectedAxis) return;
		const profileIndex = selectedProfile?.profileIndex ?? 0;
		let targets = null;
		if (combine !== 'NewBody' && targetMode === 'Choose') {
			const all = getBodies();
			targets = selectedTargetIds
				.map((id) => all.find((b) => b.bodyId === id))
				.filter(Boolean)
				.map(bodyToGeomRef);
		}
		applyRevolve(angle, [...selectedAxis.origin], [...selectedAxis.direction], profileIndex, {
			combine,
			targets
		}).catch(err => log('error', `Revolve dialog apply failed: ${err}`));
	}

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
		setRevolvePreviewParams(null);
		clearExtrudeTargets();
		hideRevolveDialog();
	}

	function toggleProfilePick() {
		if (profilePickActive) {
			setProfilePickMode(null);
		} else {
			setProfilePickMode({ target: 'revolve' });
		}
	}

	function toggleAxisPick() {
		setAxisPickMode(!axisPickActive);
	}
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="revolve-panel" data-testid="revolve-dialog">
		<div class="dialog-header">
			<span class="dialog-title">{dialogState.editingFeatureId ? 'Edit Revolve' : 'Revolve'}</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			<div class="field">
				<label for="revolve-sketch">Sketch</label>
				<span id="revolve-sketch" class="field-value">{dialogState.sketchName}</span>
			</div>
			<div class="field">
				<label for="revolve-angle">Angle</label>
				<input
					id="revolve-angle"
					type="number"
					bind:value={angle}
					step="15"
					min="0.1"
					max="360"
				/>
			</div>
			<div
				class="pick-box"
				class:active={axisPickActive}
				role="button"
				tabindex="0"
				onclick={toggleAxisPick}
				data-testid="revolve-axis-box"
			>
				<div class="pick-box-header">
					<span class="pick-box-label">Axis</span>
					<span class="pick-hint">
						{axisPickActive ? 'Click a line or edge...' : 'Click to pick'}
					</span>
				</div>
				{#if selectedAxis}
					<div class="pick-item" data-testid="revolve-axis-item">
						<span class="pick-item-label">{selectedAxis.label}</span>
					</div>
				{:else}
					<div class="pick-empty">No axis selected</div>
				{/if}
			</div>
			<div
				class="pick-box"
				class:active={profilePickActive}
				role="button"
				tabindex="0"
				onclick={toggleProfilePick}
				data-testid="revolve-profile-box"
			>
				<div class="pick-box-header">
					<span class="pick-box-label">Profile</span>
					<span class="pick-hint">
						{profilePickActive ? 'Click a sketch profile...' : 'Click to pick'}
					</span>
				</div>
				{#if selectedProfile}
					<div class="pick-item" data-testid="revolve-profile-item">
						<span class="pick-item-label">{selectedProfile.label}</span>
					</div>
				{:else}
					<div class="pick-empty">No profile selected</div>
				{/if}
			</div>
				<div class="field">
					<label for="revolve-combine">Combine</label>
					<select id="revolve-combine" data-testid="revolve-combine" bind:value={combine}>
						<option value="NewBody">New Body</option>
						<option value="Add">Add</option>
						<option value="Cut">Cut</option>
						<option value="Intersect">Intersect</option>
					</select>
				</div>
				{#if combine !== 'NewBody'}
					<div class="field">
						<label for="revolve-target-mode">Targets</label>
						<select id="revolve-target-mode" data-testid="revolve-target-mode" bind:value={targetMode}>
							<option value="Auto">Auto (bodies touching sketch)</option>
							<option value="Choose">Choose bodies…</option>
						</select>
					</div>
					{#if targetMode === 'Choose'}
						<div class="field">
							<label for="revolve-target-pick">In viewport</label>
							<button id="revolve-target-pick" class="btn" class:active={targetPickActive} data-testid="revolve-target-pick" onclick={() => setExtrudeTargetPickActive(!targetPickActive)}>
								{targetPickActive ? 'Picking… (click bodies)' : 'Pick in viewport'}
							</button>
						</div>
						<div class="target-list" data-testid="revolve-target-list">
							{#each bodies as body}
								<label class="target-item">
									<input type="checkbox" checked={selectedTargetIds.includes(body.bodyId)} onchange={() => toggleTarget(body.bodyId)} />
									<span>{body.name}</span>
								</label>
							{/each}
							{#if bodies.length === 0}
								<div class="pick-empty">No bodies yet</div>
							{/if}
						</div>
					{/if}
				{/if}
		</div>
		<div class="dialog-footer">
			<button class="btn btn-cancel" data-testid="revolve-cancel" onclick={handleCancel}>Cancel</button>
			<button
				class="btn btn-apply"
				data-testid="revolve-apply"
				disabled={!hasAxis}
				onclick={handleApply}
			>Apply</button>
		</div>
	</div>
{/if}

<style>
	.revolve-panel {
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
		.revolve-panel {
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
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.field label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		min-width: 50px;
	}

	.field-value {
		font-size: 12px;
		color: var(--text-primary, #eee);
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

	.field input:focus,
	.field select:focus {
		outline: none;
		border-color: var(--accent, #0078d4);
	}

	.pick-box {
		border: 2px solid var(--border-color, #444);
		border-radius: 4px;
		padding: 8px;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.pick-box:hover { border-color: var(--accent, #0078d4); }
	.pick-box.active {
		border-color: var(--accent, #0078d4);
		background: rgba(0, 120, 212, 0.1);
		animation: pulse-border 1.5s ease-in-out infinite;
	}

	@keyframes pulse-border {
		0%, 100% { border-color: var(--accent, #0078d4); }
		50% { border-color: rgba(0, 120, 212, 0.4); }
	}

	.pick-box-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.pick-box-label {
		font-size: 11px;
		color: var(--text-secondary, #aaa);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
	}

	.pick-hint {
		font-size: 10px;
		color: var(--text-muted, #888);
		font-style: italic;
	}

	.pick-item {
		display: flex;
		align-items: center;
		padding: 4px 8px;
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		border-radius: 3px;
		font-size: 12px;
		color: var(--text-primary, #eee);
	}

	.pick-item-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.pick-empty {
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
		border: 1px solid var(--border-color, #444);
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
