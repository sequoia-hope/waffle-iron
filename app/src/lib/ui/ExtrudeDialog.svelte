<script>
	import {
		getExtrudeDialogState,
		hideExtrudeDialog,
		applyExtrude
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let dialogState = $derived(getExtrudeDialogState());
	let depth = $state(10);
	let profileIndex = $state(0);
	let cut = $state(false);
	let depthMode = $state('Blind');
	let secondDir = $state('None');
	let secondDepth = $state(10);
	let showDirection = $state(false);
	let dirX = $state(0);
	let dirY = $state(0);
	let dirZ = $state(1);

	let showDepthInput = $derived(depthMode === 'Blind');
	let depthLabel = $derived(secondDir === 'Symmetric' ? 'Depth (each side)' : 'Depth');
	let showSecondDepthInput = $derived(secondDir === 'Blind');

	$effect(() => {
		if (dialogState) {
			depth = 10;
			profileIndex = 0;
			cut = false;
			depthMode = 'Blind';
			secondDir = 'None';
			secondDepth = 10;
			showDirection = false;
			dirX = 0;
			dirY = 0;
			dirZ = 1;
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
			direction: showDirection ? [dirX, dirY, dirZ] : null
		};
		applyExtrude(depth, profileIndex, cut, opts)
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
</script>

{#if dialogState}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="overlay" onkeydown={handleKeydown} data-testid="extrude-dialog">
		<div class="dialog">
			<div class="dialog-header">
				<span class="dialog-title">Extrude</span>
				<button class="close-btn" onclick={handleCancel}>&times;</button>
			</div>
			<div class="dialog-body">
				<div class="field">
					<label for="extrude-sketch">Sketch</label>
					<span id="extrude-sketch" class="field-value">{dialogState.sketchName}</span>
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
				{#if dialogState.profileCount > 1}
					<div class="field">
						<label for="extrude-profile">Profile</label>
						<select id="extrude-profile" bind:value={profileIndex}>
							{#each Array(dialogState.profileCount) as _, i}
								<option value={i}>Profile {i + 1}</option>
							{/each}
						</select>
					</div>
				{/if}
				<div class="field">
					<label for="extrude-dir-override">Direction</label>
					<input
						id="extrude-dir-override"
						data-testid="extrude-dir-override"
						type="checkbox"
						bind:checked={showDirection}
					/>
				</div>
				{#if showDirection}
					<div class="field-group">
						<span class="group-label">Direction Vector</span>
						<div class="vec3">
							<label>X <input type="number" data-testid="extrude-dir-x" bind:value={dirX} step="0.1" /></label>
							<label>Y <input type="number" data-testid="extrude-dir-y" bind:value={dirY} step="0.1" /></label>
							<label>Z <input type="number" data-testid="extrude-dir-z" bind:value={dirZ} step="0.1" /></label>
						</div>
					</div>
				{/if}
			</div>
			<div class="dialog-footer">
				<button class="btn btn-cancel" data-testid="extrude-cancel" onclick={handleCancel}>Cancel</button>
				<button class="btn btn-apply" data-testid="extrude-apply" onclick={handleApply}>Apply</button>
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
		min-width: 280px;
		max-width: calc(100vw - 32px);
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

	.field input[type="checkbox"] {
		width: auto;
		accent-color: var(--accent, #0078d4);
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
