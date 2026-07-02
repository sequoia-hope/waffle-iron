<script>
	import { getToasts, dismissToast } from './toast.svelte.js';

	let toasts = $derived(getToasts());
</script>

<div class="toast-container" data-testid="toast-container">
	{#each toasts as toast (toast.id)}
		<div class="toast toast-{toast.level}" data-testid="toast-item-{toast.id}">
			<span class="toast-message">{toast.message}</span>
			<button class="toast-close" data-testid="toast-close-{toast.id}" onclick={() => dismissToast(toast.id)}>&times;</button>
		</div>
	{/each}
</div>

<style>
	.toast-container {
		position: fixed;
		bottom: calc(var(--statusbar-height, 28px) + 8px);
		right: max(12px, env(safe-area-inset-right, 0px));
		z-index: 2000;
		display: flex;
		flex-direction: column;
		gap: 6px;
		pointer-events: none;
		max-width: 360px;
	}

	.toast {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		border-radius: 4px;
		font-size: 12px;
		color: var(--text-on-accent);
		pointer-events: auto;
		animation: toast-slide-in 0.25s ease-out;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
	}

	.toast-error {
		background: var(--error, #d32f2f);
	}

	.toast-warning {
		background: #e67e22;
		/* Fixed dark-orange fill in both themes → keep light text. */
		color: #fff;
	}

	.toast-info {
		background: var(--accent, #0078d4);
	}

	.toast-success {
		background: var(--success, #388e3c);
	}

	.toast-message {
		flex: 1;
		line-height: 1.3;
		overflow-wrap: break-word;
	}

	.toast-close {
		background: none;
		border: none;
		color: inherit;
		opacity: 0.8;
		font-size: 16px;
		cursor: pointer;
		padding: 0 2px;
		line-height: 1;
		flex-shrink: 0;
	}

	.toast-close:hover {
		opacity: 1;
	}

	@keyframes toast-slide-in {
		from {
			transform: translateX(100%);
			opacity: 0;
		}
		to {
			transform: translateX(0);
			opacity: 1;
		}
	}

	@media (max-width: 768px) {
		.toast-container {
			right: auto;
			left: 50%;
			transform: translateX(-50%);
			width: calc(100vw - 48px - env(safe-area-inset-left, 0px) - env(safe-area-inset-right, 0px));
			max-width: 320px;
		}
	}
</style>
