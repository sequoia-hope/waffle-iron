<script>
	import {
		getSketchPlaneDialogVisible,
		getSketchPlaneDialogSelection,
		setSketchPlaneDialogSelection,
		hideSketchPlaneDialog,
		confirmSketchPlaneDialog
	} from '$lib/engine/store.svelte.js';

	let visible = $derived(getSketchPlaneDialogVisible());
	let selection = $derived(getSketchPlaneDialogSelection());

	const planes = [
		{ label: 'XY Plane', origin: /** @type {[number,number,number]} */ ([0, 0, 0]), normal: /** @type {[number,number,number]} */ ([0, 0, 1]) },
		{ label: 'XZ Plane', origin: /** @type {[number,number,number]} */ ([0, 0, 0]), normal: /** @type {[number,number,number]} */ ([0, 1, 0]) },
		{ label: 'YZ Plane', origin: /** @type {[number,number,number]} */ ([0, 0, 0]), normal: /** @type {[number,number,number]} */ ([1, 0, 0]) },
	];

	function selectPlane(plane) {
		setSketchPlaneDialogSelection(plane);
	}

	function handleApply() {
		confirmSketchPlaneDialog();
	}

	function handleCancel() {
		hideSketchPlaneDialog();
	}

	$effect(() => {
		if (!visible) return;
		function onKeyDown(e) {
			if (e.key === 'Enter' && selection) {
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
				<span class="dialog-title">Select Sketch Plane</span>
				<button class="close-btn" onclick={handleCancel}>&times;</button>
			</div>
			<div class="dialog-body">
				<p class="hint">{selection ? selection.label : 'Choose a datum plane or click a face in the viewport'}</p>
				<div class="plane-buttons">
					{#each planes as plane}
						<button
							class="plane-btn"
							class:selected={selection?.label === plane.label}
							data-testid="plane-btn-{plane.label.replace(/\s/g, '-').toLowerCase()}"
							onclick={() => selectPlane(plane)}
						>{plane.label}</button>
					{/each}
				</div>
			</div>
			<div class="dialog-footer">
				<button class="btn btn-cancel" data-testid="sketch-plane-cancel" onclick={handleCancel}>Cancel</button>
				<button class="btn btn-apply" data-testid="sketch-plane-ok" disabled={!selection} onclick={handleApply}>OK</button>
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

	.hint {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		margin: 0;
	}

	.plane-buttons {
		display: flex;
		gap: 6px;
	}

	.plane-btn {
		flex: 1;
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
