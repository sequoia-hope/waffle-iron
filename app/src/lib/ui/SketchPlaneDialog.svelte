<script>
	import {
		getSketchPlaneDialogVisible,
		getSketchPlaneDialogSelection,
		setSketchPlaneDialogSelection,
		hideSketchPlaneDialog,
		confirmSketchPlaneDialog,
		getSelectedRefs,
		computeFacePlane,
		getFeatureTree,
		createDatumPlane
	} from '$lib/engine/store.svelte.js';
	import { getAllPlanes, resolvePlane } from '$lib/engine/planes.js';

	let visible = $derived(getSketchPlaneDialogVisible());
	let selection = $derived(getSketchPlaneDialogSelection());
	let features = $derived(getFeatureTree()?.features ?? []);

	// Reactive plane list: built-in + user-created
	let allPlanes = $derived(getAllPlanes(features).map((p) => {
		const resolved = resolvePlane(p.definition, features);
		return { id: p.id, label: p.name, origin: resolved.origin, normal: resolved.normal, builtin: p.builtin };
	}));

	// Dialog mode: 'select' or 'create-offset'
	let mode = $state('select');
	let offsetBasePlaneId = $state('');
	let offsetDistance = $state(10);
	let offsetName = $state('Offset Plane');

	// Wire face clicks into the dialog while it's visible
	$effect(() => {
		if (!visible) return;
		const refs = getSelectedRefs();
		if (refs.length === 1 && refs[0]?.kind?.type === 'Face') {
			const plane = computeFacePlane(refs[0]);
			if (plane) {
				setSketchPlaneDialogSelection({
					origin: plane.origin, normal: plane.normal, label: 'Selected Face'
				});
			}
		}
	});

	function selectPlane(plane) {
		setSketchPlaneDialogSelection(plane);
	}

	function handleApply() {
		confirmSketchPlaneDialog();
	}

	function handleCancel() {
		mode = 'select';
		hideSketchPlaneDialog();
	}

	function switchToCreateOffset() {
		// Default base plane to the first available
		if (allPlanes.length > 0 && !offsetBasePlaneId) {
			offsetBasePlaneId = allPlanes[0].id;
		}
		mode = 'create-offset';
	}

	function handleCreateOffset() {
		if (!offsetBasePlaneId) return;
		const definition = {
			method: 'offset',
			basePlaneId: offsetBasePlaneId,
			distance: offsetDistance
		};
		createDatumPlane(definition, offsetName);
		// Reset and go back to select mode
		mode = 'select';
		offsetDistance = 10;
		offsetName = 'Offset Plane';
	}

	$effect(() => {
		if (!visible) return;
		// Reset mode when dialog opens
		mode = 'select';

		function onKeyDown(e) {
			if (e.key === 'Enter' && selection && mode === 'select') {
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
</script>

{#if visible}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="overlay" data-testid="sketch-plane-dialog">
		<div class="dialog">
			<div class="dialog-header">
				<span class="dialog-title">{mode === 'select' ? 'Select Sketch Plane' : 'Create Offset Plane'}</span>
				<button class="close-btn" onclick={handleCancel}>&times;</button>
			</div>

			{#if mode === 'select'}
				<div class="dialog-body">
					<p class="hint">{selection ? selection.label : 'Choose a plane or click a face in the viewport'}</p>
					<div class="plane-buttons">
						{#each allPlanes as plane (plane.id)}
							<button
								class="plane-btn"
								class:selected={selection?.label === plane.label}
								class:user-plane={!plane.builtin}
								data-testid="plane-btn-{plane.label.toLowerCase().replace(/\s+/g, '-')}"
								onclick={() => selectPlane(plane)}
							>{plane.label}</button>
						{/each}
					</div>
					<button class="create-offset-btn" data-testid="create-offset-btn" onclick={switchToCreateOffset}>+ Offset Plane</button>
				</div>
				<div class="dialog-footer">
					<button class="btn btn-cancel" data-testid="sketch-plane-cancel" onclick={handleCancel}>Cancel</button>
					<button class="btn btn-apply" data-testid="sketch-plane-ok" disabled={!selection} onclick={handleApply}>OK</button>
				</div>

			{:else if mode === 'create-offset'}
				<div class="dialog-body">
					<label class="field-label">
						Name
						<input class="field-input" type="text" bind:value={offsetName} data-testid="offset-name-input" />
					</label>
					<label class="field-label">
						Base Plane
						<select class="field-input" bind:value={offsetBasePlaneId} data-testid="offset-base-select">
							{#each allPlanes as plane (plane.id)}
								<option value={plane.id}>{plane.label}</option>
							{/each}
						</select>
					</label>
					<label class="field-label">
						Distance
						<input class="field-input" type="number" bind:value={offsetDistance} step="1" data-testid="offset-distance-input" />
					</label>
				</div>
				<div class="dialog-footer">
					<button class="btn btn-cancel" data-testid="offset-back-btn" onclick={() => mode = 'select'}>Back</button>
					<button class="btn btn-apply" data-testid="offset-create-btn" onclick={handleCreateOffset}>Create</button>
				</div>
			{/if}
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

	.hint {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		margin: 0;
	}

	.plane-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.plane-btn {
		flex: 1;
		min-width: 70px;
		padding: 8px 4px;
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		border-radius: 4px;
		font-size: 12px;
		cursor: pointer;
	}

	.plane-btn:hover {
		border-color: var(--accent, #0078d4);
		background: var(--bg-hover, #333);
	}

	.plane-btn.selected {
		border-color: var(--accent, #0078d4);
		background: rgba(0, 120, 212, 0.2);
		color: var(--accent, #0078d4);
	}

	.plane-btn.user-plane {
		border-color: #aa8844;
	}

	.plane-btn.user-plane.selected {
		border-color: #ffcc88;
		background: rgba(170, 136, 68, 0.2);
		color: #ffcc88;
	}

	.create-offset-btn {
		padding: 6px 10px;
		background: transparent;
		border: 1px dashed var(--border-color, #555);
		color: var(--text-secondary, #aaa);
		border-radius: 4px;
		font-size: 11px;
		cursor: pointer;
	}

	.create-offset-btn:hover {
		border-color: var(--accent, #0078d4);
		color: var(--text-primary, #eee);
	}

	.field-label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 12px;
		color: var(--text-secondary, #aaa);
	}

	.field-input {
		padding: 6px 8px;
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		border-radius: 4px;
		font-size: 12px;
	}

	.field-input:focus {
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

	.btn-apply:hover:not(:disabled) {
		filter: brightness(1.1);
	}

	.btn-apply:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
