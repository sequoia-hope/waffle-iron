<script>
	import { THEMES, getTheme, setTheme } from '$lib/ui/theme.svelte.js';

	/** @type {{ compact?: boolean }} */
	let { compact = false } = $props();

	let active = $derived(getTheme());
	let open = $state(false);

	// Fixed-position dropdown: anchor the panel to the trigger's viewport rect
	// rather than a positioned ancestor, so it's never clipped by an
	// overflowing toolbar or a thin status bar. Opens upward when the trigger
	// sits in the lower half of the viewport (e.g. the bottom status bar).
	let pos = $state({ top: null, bottom: null, right: 0 });

	/** @param {MouseEvent} e */
	function toggle(e) {
		const rect = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect();
		const saiRight = parseFloat(
			getComputedStyle(document.documentElement).getPropertyValue('--sai-right')
		) || 0;
		const right = Math.max(4, Math.round(window.innerWidth - rect.right - saiRight));
		if (rect.top > window.innerHeight / 2) {
			pos = { top: null, bottom: Math.round(window.innerHeight - rect.top + 4), right };
		} else {
			pos = { top: Math.round(rect.bottom + 4), bottom: null, right };
		}
		open = !open;
	}

	/** @param {string} id */
	function choose(id) {
		setTheme(id);
		open = false;
	}

	let menuStyle = $derived(
		(pos.top != null ? `top:${pos.top}px;` : '') +
		(pos.bottom != null ? `bottom:${pos.bottom}px;` : '') +
		`right:${pos.right}px;`
	);
</script>

<div class="theme-switcher">
	<button
		class="theme-trigger"
		class:compact
		data-testid="theme-switcher-trigger"
		title="Theme"
		aria-label="Theme"
		aria-haspopup="menu"
		aria-expanded={open}
		onclick={toggle}
	>
		<!-- Palette glyph -->
		<span class="glyph" aria-hidden="true">◐</span>
	</button>
	{#if open}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="theme-backdrop" onclick={() => (open = false)}></div>
		<div
			class="theme-menu"
			role="menu"
			data-testid="theme-switcher-menu"
			style={menuStyle}
		>
			{#each THEMES as t (t.id)}
				<button
					class="theme-item"
					class:selected={t.id === active}
					role="menuitemradio"
					aria-checked={t.id === active}
					data-testid="theme-option-{t.id}"
					onclick={() => choose(t.id)}
				>
					<span class="check">{t.id === active ? '✓' : ''}</span>
					<span class="labels">
						<span class="label">{t.label}</span>
						<span class="desc">{t.description}</span>
					</span>
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.theme-switcher {
		display: flex;
		align-items: center;
		flex: 0 0 auto;
	}

	.theme-trigger {
		background: transparent;
		border: 1px solid transparent;
		color: var(--text-primary);
		padding: 4px 6px;
		border-radius: 3px;
		cursor: pointer;
		font-size: 14px;
		line-height: 1;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.theme-trigger:hover {
		background: var(--bg-hover);
	}

	/* Compact variant for the thin status bar: inherits the bar's on-accent
	   color, no hover fill (the bar is already a saturated fill). */
	.theme-trigger.compact {
		padding: 0 4px;
		font-size: 13px;
		color: inherit;
	}

	.theme-trigger.compact:hover {
		background: color-mix(in srgb, var(--text-on-accent) 18%, transparent);
	}

	.glyph {
		display: inline-block;
	}

	.theme-backdrop {
		position: fixed;
		inset: 0;
		z-index: 199;
	}

	.theme-menu {
		position: fixed;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		z-index: 200;
		min-width: 220px;
		max-width: calc(100vw - 16px - env(safe-area-inset-left, 0px) - env(safe-area-inset-right, 0px));
		padding: 4px 0;
	}

	.theme-item {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		width: 100%;
		background: none;
		border: none;
		color: var(--text-primary);
		padding: 8px 12px;
		font-size: 13px;
		text-align: left;
		cursor: pointer;
	}

	.theme-item:hover {
		background: var(--bg-hover);
	}

	.theme-item.selected {
		color: var(--accent);
	}

	.check {
		width: 12px;
		flex: 0 0 auto;
		color: var(--accent);
	}

	.labels {
		display: flex;
		flex-direction: column;
	}

	.label {
		white-space: nowrap;
	}

	.desc {
		color: var(--text-muted);
		font-size: 11px;
		white-space: normal;
	}
</style>
