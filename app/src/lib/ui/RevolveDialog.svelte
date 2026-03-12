<script>
	import {
		getRevolveDialogState,
		hideRevolveDialog,
		applyRevolve,
		setRevolvePreviewParams
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getRevolveDialogState());
	let angle = $state(360);
	let selectedAxisId = $state(null);
	let axisOrigin = $state([0, 0, 0]);
	let axisDir = $state([0, 1, 0]);
	let profileIndex = $state(0);

	// Compute plane basis vectors from normal
	function computePlaneBasis(pn) {
		const absX = Math.abs(pn[0]), absY = Math.abs(pn[1]), absZ = Math.abs(pn[2]);
		let upHint;
		if (absZ >= absX && absZ >= absY) {
			upHint = [0, 1, 0];
		} else if (absY >= absX) {
			upHint = [0, 0, 1];
		} else {
			upHint = [0, 1, 0];
		}

		const rx = upHint[1] * pn[2] - upHint[2] * pn[1];
		const ry = upHint[2] * pn[0] - upHint[0] * pn[2];
		const rz = upHint[0] * pn[1] - upHint[1] * pn[0];
		const rlen = Math.sqrt(rx*rx + ry*ry + rz*rz);
		const right = rlen > 1e-10 ? [rx/rlen, ry/rlen, rz/rlen] : [1, 0, 0];

		const ux = pn[1] * right[2] - pn[2] * right[1];
		const uy = pn[2] * right[0] - pn[0] * right[2];
		const uz = pn[0] * right[1] - pn[1] * right[0];
		const up = [ux, uy, uz];

		return { right, up };
	}

	// Compute 3D axis from a selected entity
	function computeAxisFromEntity(entity, state) {
		const pn = state.planeNormal ?? [0, 0, 1];
		const po = state.planeOrigin ?? [0, 0, 0];
		const { right, up } = computePlaneBasis(pn);

		if (entity.type === 'Line') {
			const dx2d = entity.end[0] - entity.start[0];
			const dy2d = entity.end[1] - entity.start[1];
			const len = Math.sqrt(dx2d * dx2d + dy2d * dy2d);
			if (len < 1e-10) return null;

			const nx = dx2d / len;
			const ny = dy2d / len;

			return {
				dir: [
					right[0] * nx + up[0] * ny,
					right[1] * nx + up[1] * ny,
					right[2] * nx + up[2] * ny
				],
				origin: [
					po[0] + right[0] * entity.start[0] + up[0] * entity.start[1],
					po[1] + right[1] * entity.start[0] + up[1] * entity.start[1],
					po[2] + right[2] * entity.start[0] + up[2] * entity.start[1]
				]
			};
		} else if (entity.type === 'Circle') {
			return {
				dir: [pn[0], pn[1], pn[2]],
				origin: [
					po[0] + right[0] * entity.center[0] + up[0] * entity.center[1],
					po[1] + right[1] * entity.center[0] + up[1] * entity.center[1],
					po[2] + right[2] * entity.center[0] + up[2] * entity.center[1]
				]
			};
		}
		return null;
	}

	function selectAxis(entityId) {
		if (!dialogState) return;
		const entity = dialogState.axisEntities?.find(e => e.id === entityId);
		if (!entity) return;

		selectedAxisId = entityId;
		const result = computeAxisFromEntity(entity, dialogState);
		if (result) {
			axisDir = result.dir;
			axisOrigin = result.origin;
		}
	}

	// Reset state and auto-select on dialog open
	$effect(() => {
		if (dialogState) {
			angle = 360;
			profileIndex = 0;
			selectedAxisId = null;
			axisOrigin = [0, 0, 0];
			axisDir = [0, 1, 0];

			// Auto-select: first construction line, then first line, then first circle
			const entities = dialogState.axisEntities ?? [];
			const pick =
				entities.find(e => e.type === 'Line' && e.construction) ??
				entities.find(e => e.type === 'Line') ??
				entities.find(e => e.type === 'Circle');
			if (pick) {
				selectAxis(pick.id);
			}
		}
	});

	// Send preview params whenever axis/angle/profile changes
	$effect(() => {
		if (dialogState) {
			setRevolvePreviewParams({
				sketchId: dialogState.sketchId,
				profileIndex,
				angle,
				axisOrigin: [...axisOrigin],
				axisDir: [...axisDir]
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

	let hasAxis = $derived(selectedAxisId != null);

	function handleApply() {
		if (!hasAxis) return;
		applyRevolve(angle, [...axisOrigin], [...axisDir], profileIndex)
			.catch(err => log('error', `Revolve dialog apply failed: ${err}`));
	}

	function handleCancel() {
		setRevolvePreviewParams(null);
		hideRevolveDialog();
	}

	function entityLabel(entity) {
		const label = entity.type === 'Circle'
			? `Circle ${entity.id}`
			: `Line ${entity.id}`;
		return entity.construction ? label + ' (constr.)' : label;
	}
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="revolve-panel" data-testid="revolve-dialog">
		<div class="dialog-header">
			<span class="dialog-title">Revolve</span>
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
			<div class="field-group">
				<span class="group-label">Axis</span>
				{#if dialogState.axisEntities?.length > 0}
					<div class="axis-entity-list" data-testid="revolve-axis-list">
						{#each dialogState.axisEntities as entity (entity.id)}
							<button
								class="btn-axis-entity"
								class:active={selectedAxisId === entity.id}
								class:construction={entity.construction}
								data-testid="revolve-axis-entity-{entity.id}"
								onclick={() => selectAxis(entity.id)}
							>{entityLabel(entity)}</button>
						{/each}
					</div>
				{:else}
					<span class="no-entities">No lines or circles in sketch</span>
				{/if}
			</div>
			{#if dialogState.profileCount > 1}
				<div class="field">
					<label for="revolve-profile">Profile</label>
					<select id="revolve-profile" bind:value={profileIndex}>
						{#each Array(dialogState.profileCount) as _, i}
							<option value={i}>Profile {i + 1}</option>
						{/each}
					</select>
				</div>
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

	.field-group {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.group-label {
		font-size: 11px;
		color: var(--text-secondary, #aaa);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
	}

	.axis-entity-list {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.btn-axis-entity {
		padding: 4px 8px;
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
		text-align: left;
	}

	.btn-axis-entity:hover {
		border-color: var(--accent, #0078d4);
	}

	.btn-axis-entity.active {
		background: var(--accent, #0078d4);
		border-color: var(--accent, #0078d4);
		color: #fff;
	}

	.btn-axis-entity.construction {
		font-style: italic;
	}

	.no-entities {
		font-size: 11px;
		color: var(--text-muted, #888);
		font-style: italic;
		padding: 4px 0;
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
