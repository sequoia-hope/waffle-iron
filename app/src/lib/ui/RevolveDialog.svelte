<script>
	import {
		getRevolveDialogState,
		hideRevolveDialog,
		applyRevolve
	} from '$lib/engine/store.svelte.js';

	let dialogState = $derived(getRevolveDialogState());
	let angle = $state(360);
	let axisOriginX = $state(0);
	let axisOriginY = $state(0);
	let axisOriginZ = $state(0);
	let axisDirX = $state(0);
	let axisDirY = $state(1);
	let axisDirZ = $state(0);
	let profileIndex = $state(0);

	$effect(() => {
		if (dialogState) {
			angle = 360;
			axisOriginX = 0;
			axisOriginY = 0;
			axisOriginZ = 0;
			axisDirX = 0;
			axisDirY = 1;
			axisDirZ = 0;
			profileIndex = 0;
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
		applyRevolve(
			angle,
			[axisOriginX, axisOriginY, axisOriginZ],
			[axisDirX, axisDirY, axisDirZ],
			profileIndex
		).catch(() => {});
	}

	function handleCancel() {
		hideRevolveDialog();
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
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="overlay" onkeydown={handleKeydown} data-testid="revolve-dialog">
		<div class="dialog">
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
					<label for="revolve-angle">Angle (&deg;)</label>
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
					<span class="group-label">Axis Origin</span>
					<div class="vec3">
						<label>X <input type="number" bind:value={axisOriginX} step="1" /></label>
						<label>Y <input type="number" bind:value={axisOriginY} step="1" /></label>
						<label>Z <input type="number" bind:value={axisOriginZ} step="1" /></label>
					</div>
				</div>
				<div class="field-group">
					<span class="group-label">Axis Direction</span>
					<div class="vec3">
						<label>X <input type="number" bind:value={axisDirX} step="0.1" /></label>
						<label>Y <input type="number" bind:value={axisDirY} step="0.1" /></label>
						<label>Z <input type="number" bind:value={axisDirZ} step="0.1" /></label>
					</div>
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
				<button class="btn btn-apply" data-testid="revolve-apply" onclick={handleApply}>Apply</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		z-index: 1000;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(0, 0, 0, 0.3);
	}

	.dialog {
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		min-width: 300px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
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
		min-width: 70px;
	}

	.field-value {
		font-size: 12px;
		color: var(--text-primary, #eee);
	}

	.field input,
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
		font-size: 12px;
		color: var(--text-secondary, #aaa);
	}

	.vec3 {
		display: flex;
		gap: 6px;
	}

	.vec3 label {
		display: flex;
		align-items: center;
		gap: 2px;
		font-size: 11px;
		color: var(--text-muted, #888);
	}

	.vec3 input {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 3px 5px;
		border-radius: 3px;
		font-size: 11px;
		width: 55px;
	}

	.vec3 input:focus {
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
		color: #fff;
		border-color: var(--accent, #0078d4);
	}

	.btn-apply:hover {
		filter: brightness(1.1);
	}
</style>
