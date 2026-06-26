<script>
	/**
	 * Constraint-modal panel. Visible while a constraint-first pick loop is
	 * active (see /specs/constraint_modal.md). Shows the active constraint, the
	 * current instruction / transient hint, and a Done button. The actual picks
	 * happen in the viewport via the 'constraint' sketch tool; this panel is the
	 * non-blocking modal chrome.
	 */
	import { getConstraintModal, closeConstraintModal } from '$lib/engine/store.svelte.js';
	import { CONSTRAINT_MODAL_SPECS } from './constraintModalEngine.js';

	let modal = $derived(getConstraintModal());
	let spec = $derived(modal ? CONSTRAINT_MODAL_SPECS[modal.constraintId] : null);
</script>

{#if modal && spec}
	<div class="constraint-modal" data-testid="constraint-modal" role="dialog" aria-label="Constraint tool">
		<div class="cm-header">
			<span class="cm-title" data-testid="constraint-modal-title">{spec.label}</span>
			<button
				class="cm-close"
				data-testid="constraint-modal-done"
				title="Done (Esc)"
				onclick={() => closeConstraintModal()}
			>Done</button>
		</div>
		<div class="cm-message" data-testid="constraint-modal-message">{modal.message ?? ''}</div>
	</div>
{/if}

<style>
	.constraint-modal {
		position: absolute;
		top: 12px;
		left: 50%;
		transform: translateX(-50%);
		z-index: 900;
		background: var(--bg-secondary, #252526);
		border: 1px solid var(--accent, #44aaff);
		border-radius: 6px;
		padding: 8px 12px;
		min-width: 220px;
		box-shadow: 0 4px 14px rgba(0, 0, 0, 0.5);
		pointer-events: auto;
	}
	.cm-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.cm-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary, #cccccc);
	}
	.cm-close {
		background: var(--accent, #44aaff);
		border: none;
		border-radius: 4px;
		color: #07121c;
		font-size: 12px;
		font-weight: 600;
		padding: 3px 10px;
		cursor: pointer;
	}
	.cm-close:hover {
		filter: brightness(1.1);
	}
	.cm-message {
		margin-top: 4px;
		font-size: 12px;
		color: var(--text-secondary, #9aa0a6);
		min-height: 15px;
	}
</style>
