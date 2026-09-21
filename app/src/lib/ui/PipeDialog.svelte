<script>
	import {
		getPipeDialogState,
		hidePipeDialog,
		applyPipe,
		setPathPickMode,
		getPathPickMode,
		togglePipePathEntity,
		getBodies,
		getExtrudeTargetPick,
		setExtrudeTargetPickActive,
		setExtrudeTargetIds,
		toggleExtrudeTargetId,
		clearExtrudeTargets,
		evaluateExpression,
		getDocumentDisplayUnit
	} from '$lib/engine/store.svelte.js';
	import { showToast } from '$lib/ui/toast.svelte.js';
	import { log } from '$lib/engine/logger.js';
	import { parseAndConvert, formatForInput, isPlainMeasurement, UNITS } from '$lib/units.js';

	// Pipe sweep dialog (spec `specs/b2_pipe_sweep.md` checkpoint 3): pick an
	// open chain of sketch lines/arcs in the viewport (one click selects the
	// whole connected chain), set the tube radius and an optional wall, and
	// the engine sweeps ONE solid. Mirrors the revolve dialog's shape.
	let dialogState = $derived(getPipeDialogState());
	let displayUnit = $derived(getDocumentDisplayUnit());
	let unitLabel = $derived(UNITS[displayUnit]?.label ?? displayUnit);

	// Radius / wall inputs: a plain measurement in the display unit or an
	// expression over the design variables (mm-space).
	let radiusInput = $state('5');
	let wallInput = $state('');
	let radiusIsExpr = $derived(radiusInput.trim() !== '' && !isPlainMeasurement(radiusInput));
	let wallIsExpr = $derived(wallInput.trim() !== '' && !isPlainMeasurement(wallInput));
	let radiusEval = $state({ value: null, error: null });
	let wallEval = $state({ value: null, error: null });
	let radiusEvalToken = 0;
	let wallEvalToken = 0;
	$effect(() => {
		const text = radiusInput.trim();
		if (!radiusIsExpr) {
			radiusEval = { value: null, error: null };
			return;
		}
		const token = ++radiusEvalToken;
		evaluateExpression(text).then((result) => {
			if (token === radiusEvalToken) radiusEval = result;
		});
	});
	$effect(() => {
		const text = wallInput.trim();
		if (!wallIsExpr) {
			wallEval = { value: null, error: null };
			return;
		}
		const token = ++wallEvalToken;
		evaluateExpression(text).then((result) => {
			if (token === wallEvalToken) wallEval = result;
		});
	});
	// Internal (meters).
	let radius = $derived(
		radiusIsExpr
			? (radiusEval.value != null ? radiusEval.value * 0.001 : NaN)
			: parseAndConvert(radiusInput, displayUnit)
	);
	let wall = $derived(
		wallInput.trim() === ''
			? null
			: wallIsExpr
				? (wallEval.value != null ? wallEval.value * 0.001 : NaN)
				: parseAndConvert(wallInput, displayUnit)
	);

	let combine = $state('NewBody');
	let targetMode = $state('Auto');
	let selectedTargetIds = $derived(getExtrudeTargetPick().ids);
	let targetPickActive = $derived(getExtrudeTargetPick().active);
	let bodies = $derived(getBodies());

	let pathEntities = $derived(dialogState?.entityIds ?? []);
	let pathPickActive = $derived(getPathPickMode());

	let prevDialogOpen = false;
	$effect(() => {
		const isOpen = !!dialogState;
		if (isOpen && !prevDialogOpen) {
			const ep = dialogState.editParams;
			if (ep) {
				radiusInput = ep.radius_expr ?? formatForInput(ep.radius, displayUnit);
				if (ep.inner_radius_expr) {
					wallInput = ep.inner_radius_expr;
				} else if (ep.inner_radius != null) {
					wallInput = formatForInput(ep.radius - ep.inner_radius, displayUnit);
				} else {
					wallInput = '';
				}
				combine = ep.combine?.type ?? 'NewBody';
				if (Array.isArray(ep.targets) && ep.targets.length > 0) {
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
				radiusInput = '5';
				wallInput = '';
				combine = 'NewBody';
				targetMode = 'Auto';
				clearExtrudeTargets();
			}
			setPathPickMode(true);
		} else if (!isOpen && prevDialogOpen) {
			setPathPickMode(false);
		}
		prevDialogOpen = isOpen;
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

	let hasPath = $derived(pathEntities.length > 0);
	let radiusValid = $derived(!isNaN(radius) && radius > 0);
	let wallValid = $derived(wall === null || (!isNaN(wall) && wall > 0 && wall < radius));
	let canApply = $derived(hasPath && radiusValid && wallValid);

	$effect(() => {
		if ((combine === 'NewBody' || targetMode !== 'Choose') && targetPickActive) {
			setExtrudeTargetPickActive(false);
		}
	});

	function handleApply() {
		if (!canApply) return;
		if (radiusIsExpr && (radiusEval.error != null || radiusEval.value == null)) {
			showToast('error', `Radius expression: ${radiusEval.error ?? 'still evaluating'}`);
			return;
		}
		if (wallIsExpr && (wallEval.error != null || wallEval.value == null)) {
			showToast('error', `Wall expression: ${wallEval.error ?? 'still evaluating'}`);
			return;
		}
		let targets = null;
		if (combine !== 'NewBody' && targetMode === 'Choose') {
			const all = getBodies();
			targets = selectedTargetIds
				.map((id) => all.find((b) => b.bodyId === id))
				.filter(Boolean)
				.map(bodyToGeomRef);
		}
		applyPipe(radius, wall === null ? null : radius - wall, {
			combine,
			targets,
			radiusExpr: radiusIsExpr ? radiusInput.trim() : null,
			// The wall expression is stored as the bore radius expression.
			innerRadiusExpr: wallIsExpr ? `(${radiusInput.trim()}) - (${wallInput.trim()})` : null
		}).catch((err) => log('error', `Pipe dialog apply failed: ${err}`));
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

	function handleCancel() {
		clearExtrudeTargets();
		hidePipeDialog();
	}

	function togglePathPick() {
		setPathPickMode(!pathPickActive);
	}

	function removeEntity(id) {
		if (!dialogState) return;
		togglePipePathEntity(dialogState.sketchId, id, { expand: false });
	}
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="pipe-panel" data-testid="pipe-dialog">
		<div class="dialog-header">
			<span class="dialog-title">{dialogState.editingFeatureId ? 'Edit Pipe' : 'Pipe'}</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			<div class="field">
				<label for="pipe-sketch">Sketch</label>
				<span id="pipe-sketch" class="field-value">{dialogState.sketchName}</span>
			</div>
			<div
				class="pick-box"
				class:active={pathPickActive}
				role="button"
				tabindex="0"
				onclick={togglePathPick}
				data-testid="pipe-path-box"
			>
				<div class="pick-box-header">
					<span class="pick-box-label">Path</span>
					<span class="pick-hint">
						{pathPickActive ? 'Click a sketch line or arc...' : 'Click to pick'}
					</span>
				</div>
				{#if pathEntities.length > 0}
					<div class="pick-item" data-testid="pipe-path-item">
						<span class="pick-item-label">{pathEntities.length} segment{pathEntities.length === 1 ? '' : 's'}: {pathEntities.join(', ')}</span>
						<button
							class="pick-clear"
							data-testid="pipe-path-clear"
							onclick={(e) => { e.stopPropagation(); for (const id of [...pathEntities]) removeEntity(id); }}
							title="Clear"
						>&times;</button>
					</div>
				{:else}
					<div class="pick-empty">No path selected</div>
				{/if}
			</div>
			<div class="field">
				<label for="pipe-radius">Radius ({unitLabel})</label>
				<input
					id="pipe-radius"
					data-testid="pipe-radius"
					type="text"
					bind:value={radiusInput}
					placeholder="5"
				/>
				{#if radiusIsExpr}
					<div class="expr-hint" class:expr-error={!!radiusEval.error} data-testid="pipe-radius-eval">
						{radiusEval.error ? radiusEval.error : radiusEval.value != null ? `= ${parseFloat(radiusEval.value.toFixed(4))} mm` : '…'}
					</div>
				{/if}
			</div>
			<div class="field">
				<label for="pipe-wall">Wall ({unitLabel})</label>
				<input
					id="pipe-wall"
					data-testid="pipe-wall"
					type="text"
					bind:value={wallInput}
					placeholder="solid"
				/>
				{#if wallIsExpr}
					<div class="expr-hint" class:expr-error={!!wallEval.error} data-testid="pipe-wall-eval">
						{wallEval.error ? wallEval.error : wallEval.value != null ? `= ${parseFloat(wallEval.value.toFixed(4))} mm` : '…'}
					</div>
				{/if}
			</div>
			{#if !wallValid}
				<div class="expr-hint expr-error" data-testid="pipe-wall-error">Wall must be between 0 and the radius</div>
			{/if}
			<div class="field">
				<label for="pipe-combine">Combine</label>
				<select id="pipe-combine" data-testid="pipe-combine" bind:value={combine}>
					<option value="NewBody">New Body</option>
					<option value="Add">Add</option>
					<option value="Cut">Cut</option>
					<option value="Intersect">Intersect</option>
				</select>
			</div>
			{#if combine !== 'NewBody'}
				<div class="field">
					<label for="pipe-target-mode">Targets</label>
					<select id="pipe-target-mode" data-testid="pipe-target-mode" bind:value={targetMode}>
						<option value="Auto">Auto (most recent body)</option>
						<option value="Choose">Choose bodies…</option>
					</select>
				</div>
				{#if targetMode === 'Choose'}
					<div class="field">
						<label for="pipe-target-pick">In viewport</label>
						<button id="pipe-target-pick" class="btn" class:active={targetPickActive} data-testid="pipe-target-pick" onclick={() => setExtrudeTargetPickActive(!targetPickActive)}>
							{targetPickActive ? 'Picking… (click bodies)' : 'Pick in viewport'}
						</button>
					</div>
					<div class="target-list" data-testid="pipe-target-list">
						{#each bodies as body}
							<label class="target-item">
								<input type="checkbox" checked={selectedTargetIds.includes(body.bodyId)} onchange={() => toggleExtrudeTargetId(body.bodyId)} />
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
			<button class="btn btn-cancel" data-testid="pipe-cancel" onclick={handleCancel}>Cancel</button>
			<button
				class="btn btn-apply"
				data-testid="pipe-apply"
				disabled={!canApply}
				onclick={handleApply}
			>Apply</button>
		</div>
	</div>
{/if}

<style>
	.expr-hint {
		margin-top: 2px;
		font-size: 10px;
		font-family: ui-monospace, monospace;
		color: var(--text-secondary, #8a8);
	}

	.expr-error {
		color: var(--error-color, #f66);
	}

	.pipe-panel {
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
		.pipe-panel {
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
		flex-wrap: wrap;
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
		justify-content: space-between;
		gap: 6px;
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

	.pick-clear {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		cursor: pointer;
		font-size: 14px;
		line-height: 1;
		padding: 0 2px;
	}
	.pick-clear:hover { color: var(--text-primary, #eee); }

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
