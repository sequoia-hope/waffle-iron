<script>
	/**
	 * Application settings: a large modal with a section list on the left and
	 * the active section's controls on the right. Sections: General (workflow
	 * toggles), Sketch, Appearance (base theme + per-token color overrides +
	 * copy/paste of a complete color scheme).
	 */
	import {
		getSettings,
		updateSettings,
		setColorOverride,
		clearColorOverrides,
		resetSettings,
		exportColorScheme,
		importColorScheme,
		readTokenValue,
		getColorVersion,
		COLOR_TOKENS
	} from './settings.svelte.js';
	import { THEMES, getTheme, setTheme } from './theme.svelte.js';
	import { bumpColorVersion } from './settings.svelte.js';
	import { showToast } from './toast.svelte.js';

	/** @type {{ onclose: () => void }} */
	let { onclose } = $props();

	const SECTIONS = [
		{ id: 'general', label: 'General' },
		{ id: 'sketch', label: 'Sketch' },
		{ id: 'appearance', label: 'Appearance' },
	];
	let section = $state('general');

	let settings = $derived(getSettings());
	let theme = $derived(getTheme());
	let colorVersion = $derived(getColorVersion());

	/**
	 * Effective value of a token as shown in the editor: the override if set,
	 * else the base theme's computed value. Depends on colorVersion + theme so
	 * it re-reads after a theme switch or an override change.
	 * @param {string} id
	 */
	function effective(id) {
		void colorVersion; void theme;
		return settings.colors[id] || toHex(readTokenValue(id));
	}

	/** Normalize a computed color (may be rgb()) to #rrggbb for <input type=color>. */
	function toHex(v) {
		if (!v) return '#000000';
		v = v.trim();
		if (/^#[0-9a-f]{6}$/i.test(v)) return v.toLowerCase();
		if (/^#[0-9a-f]{3}$/i.test(v)) return ('#' + v[1] + v[1] + v[2] + v[2] + v[3] + v[3]).toLowerCase();
		const m = v.match(/rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/);
		if (m) return '#' + [m[1], m[2], m[3]].map((n) => Number(n).toString(16).padStart(2, '0')).join('');
		return '#000000';
	}

	function chooseTheme(id) {
		setTheme(id);
		// Overrides are per-token; switching the base theme re-samples the rest.
		bumpColorVersion();
	}

	// ---- Copy / paste of a complete scheme ----
	let schemeText = $state('');
	let pasteError = $state('');

	function refreshSchemeText() {
		schemeText = exportColorScheme();
	}

	async function copyScheme() {
		refreshSchemeText();
		try {
			await navigator.clipboard.writeText(schemeText);
			showToast('success', 'Color scheme copied to clipboard');
		} catch {
			showToast('info', 'Clipboard unavailable — copy the text from the box below');
		}
	}

	function applyPasted() {
		pasteError = '';
		const err = importColorScheme(schemeText);
		if (err) {
			pasteError = err;
			return;
		}
		showToast('success', 'Color scheme applied');
	}

	async function pasteFromClipboard() {
		try {
			schemeText = await navigator.clipboard.readText();
			applyPasted();
		} catch {
			showToast('info', 'Clipboard unavailable — paste into the text box, then Apply');
		}
	}

	function onKeyDown(e) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			onclose();
		}
	}

	let overrideCount = $derived(Object.keys(settings.colors).length);
</script>

<svelte:window onkeydown={onKeyDown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="settings-backdrop" data-testid="settings-backdrop" onclick={onclose}></div>
<div class="settings-modal" role="dialog" aria-modal="true" aria-label="Settings" data-testid="settings-modal">
	<div class="settings-header">
		<span class="settings-title">Settings</span>
		<button class="close-btn" title="Close (Esc)" aria-label="Close" data-testid="settings-close" onclick={onclose}>×</button>
	</div>
	<div class="settings-body">
		<nav class="settings-nav">
			{#each SECTIONS as s (s.id)}
				<button
					class="nav-item"
					class:active={section === s.id}
					data-testid="settings-section-{s.id}"
					onclick={() => (section = s.id)}
				>{s.label}</button>
			{/each}
			<div class="nav-spacer"></div>
			<button class="nav-item danger" data-testid="settings-reset-all" onclick={() => { resetSettings(); showToast('info', 'Settings reset to defaults'); }}>
				Reset all
			</button>
		</nav>

		<div class="settings-content">
			{#if section === 'general'}
				<h2>General</h2>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-extrude-auto-select"
						checked={settings.extrudeAutoSelectRegion}
						onchange={(e) => updateSettings({ extrudeAutoSelectRegion: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Auto-select a region when opening Extrude</span>
						<span class="row-desc">Pre-selects a region of the most recent sketch. Off by default: the dialog opens in pick mode and waits for you to click the region to extrude.</span>
					</span>
				</label>

			{:else if section === 'sketch'}
				<h2>Sketch</h2>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-sketch-scale-first-dim"
						checked={settings.sketchScaleOnFirstDimension}
						onchange={(e) => updateSettings({ sketchScaleOnFirstDimension: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">First dimension scales the whole sketch</span>
						<span class="row-desc">When the first driving dimension is added to an undimensioned sketch, all geometry is scaled proportionally about the origin so the sketch keeps the shape you drew.</span>
					</span>
				</label>

			{:else if section === 'appearance'}
				<h2>Appearance</h2>

				<h3>Base theme</h3>
				<div class="theme-grid">
					{#each THEMES as t (t.id)}
						<button
							class="theme-card"
							class:selected={t.id === theme}
							data-testid="settings-theme-{t.id}"
							onclick={() => chooseTheme(t.id)}
						>
							<span class="theme-name">{t.label}</span>
							<span class="theme-desc">{t.description}</span>
						</button>
					{/each}
				</div>

				<h3>
					Colors
					<span class="h3-note">{overrideCount ? `${overrideCount} customized` : 'using theme defaults'}</span>
					{#if overrideCount}
						<button class="small-btn" data-testid="settings-clear-colors" onclick={clearColorOverrides}>Reset colors</button>
					{/if}
				</h3>
				{#each COLOR_TOKENS as group (group.group)}
					<div class="color-group">
						<div class="color-group-title">{group.group}</div>
						<div class="color-grid">
							{#each group.tokens as tok (tok.id)}
								<div class="color-row" class:overridden={!!settings.colors[tok.id]}>
									<input
										type="color"
										class="swatch"
										data-testid="color-{tok.id.slice(2)}"
										value={effective(tok.id)}
										oninput={(e) => setColorOverride(tok.id, e.currentTarget.value)}
									/>
									<span class="color-label">{tok.label}</span>
									<code class="color-hex">{effective(tok.id)}</code>
									{#if settings.colors[tok.id]}
										<button class="reset-one" title="Back to theme default" onclick={() => setColorOverride(tok.id, null)}>↺</button>
									{/if}
								</div>
							{/each}
						</div>
					</div>
				{/each}

				<h3>Copy &amp; paste a complete scheme</h3>
				<p class="hint">The scheme lists the base theme and every color, so pasting it elsewhere reproduces this look exactly.</p>
				<div class="scheme-actions">
					<button class="small-btn" data-testid="scheme-copy" onclick={copyScheme}>Copy scheme</button>
					<button class="small-btn" data-testid="scheme-paste" onclick={pasteFromClipboard}>Paste from clipboard</button>
					<button class="small-btn" data-testid="scheme-show" onclick={refreshSchemeText}>Show current</button>
					<button class="small-btn primary" data-testid="scheme-apply" onclick={applyPasted} disabled={!schemeText.trim()}>Apply text</button>
				</div>
				<textarea
					class="scheme-text"
					data-testid="scheme-text"
					rows="8"
					spellcheck="false"
					placeholder="Paste a color scheme here, or click Show current"
					bind:value={schemeText}
				></textarea>
				{#if pasteError}
					<div class="paste-error" data-testid="scheme-error">{pasteError}</div>
				{/if}
			{/if}
		</div>
	</div>
</div>

<style>
	.settings-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		z-index: 1500;
	}
	.settings-modal {
		position: fixed;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(960px, calc(100vw - 32px));
		height: min(720px, calc(100vh - 32px));
		display: flex;
		flex-direction: column;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 8px;
		box-shadow: 0 12px 48px rgba(0, 0, 0, 0.5);
		z-index: 1501;
		font-family: var(--font-ui);
		overflow: hidden;
	}
	.settings-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border-color);
		background: var(--bg-secondary);
	}
	.settings-title { font-size: 15px; font-weight: 600; }
	.close-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		font-size: 22px;
		line-height: 1;
		cursor: pointer;
		padding: 0 4px;
	}
	.close-btn:hover { color: var(--text-primary); }
	.settings-body { display: flex; flex: 1; min-height: 0; }
	.settings-nav {
		width: 170px;
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 12px 8px;
		border-right: 1px solid var(--border-color);
		background: var(--bg-secondary);
	}
	.nav-item {
		text-align: left;
		background: none;
		border: none;
		color: var(--text-secondary);
		padding: 8px 10px;
		border-radius: 4px;
		cursor: pointer;
		font-size: 13px;
	}
	.nav-item:hover { background: var(--bg-hover); color: var(--text-primary); }
	.nav-item.active { background: var(--accent); color: var(--text-on-accent); }
	.nav-item.danger { color: var(--error); }
	.nav-spacer { flex: 1; }
	.settings-content {
		flex: 1;
		overflow-y: auto;
		padding: 16px 24px 32px;
	}
	h2 { font-size: 16px; margin: 0 0 12px; font-weight: 600; }
	h3 {
		font-size: 13px;
		margin: 20px 0 8px;
		font-weight: 600;
		color: var(--text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.h3-note { font-weight: 400; text-transform: none; letter-spacing: 0; color: var(--text-muted); }
	.row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 0;
		border-bottom: 1px solid var(--border-color);
		cursor: pointer;
	}
	.row input[type='checkbox'] { margin-top: 3px; accent-color: var(--accent); }
	.row-text { display: flex; flex-direction: column; gap: 3px; }
	.row-label { font-size: 13px; }
	.row-desc { font-size: 12px; color: var(--text-secondary); line-height: 1.4; }
	.hint { font-size: 12px; color: var(--text-secondary); margin: 0 0 8px; }

	.theme-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 8px; }
	.theme-card {
		text-align: left;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		padding: 10px 12px;
		cursor: pointer;
		color: var(--text-primary);
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.theme-card:hover { border-color: var(--accent); }
	.theme-card.selected { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent); }
	.theme-name { font-size: 13px; font-weight: 600; }
	.theme-desc { font-size: 11px; color: var(--text-secondary); }

	.color-group { margin-bottom: 12px; }
	.color-group-title { font-size: 12px; color: var(--text-muted); margin: 6px 0 4px; }
	.color-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 4px 16px; }
	.color-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 3px 4px;
		border-radius: 4px;
	}
	.color-row.overridden { background: var(--bg-tertiary); }
	.swatch {
		width: 28px;
		height: 22px;
		padding: 0;
		border: 1px solid var(--border-color);
		border-radius: 3px;
		background: none;
		cursor: pointer;
	}
	.color-label { flex: 1; font-size: 12px; }
	.color-hex { font-family: var(--font-mono); font-size: 11px; color: var(--text-muted); }
	.reset-one {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 13px;
		padding: 0 2px;
	}
	.reset-one:hover { color: var(--text-primary); }

	.scheme-actions { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 6px; }
	.small-btn {
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		color: var(--text-primary);
		border-radius: 4px;
		padding: 4px 10px;
		font-size: 12px;
		cursor: pointer;
	}
	.small-btn:hover { background: var(--bg-hover); }
	.small-btn.primary { background: var(--accent); border-color: var(--accent); color: var(--text-on-accent); }
	.small-btn:disabled { opacity: 0.5; cursor: default; }
	.scheme-text {
		width: 100%;
		box-sizing: border-box;
		background: var(--bg-secondary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 4px;
		font-family: var(--font-mono);
		font-size: 11px;
		padding: 8px;
		resize: vertical;
	}
	.paste-error { color: var(--error); font-size: 12px; margin-top: 4px; }

	@media (max-width: 768px) {
		.settings-modal { width: calc(100vw - 8px); height: calc(100vh - 8px); }
		.settings-nav { width: 110px; }
		.settings-content { padding: 12px; }
	}
</style>
