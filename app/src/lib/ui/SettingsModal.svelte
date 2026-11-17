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
	import { onDestroy } from 'svelte';
	import { THEMES, getTheme, setTheme } from './theme.svelte.js';
	import { bumpColorVersion } from './settings.svelte.js';
	import { showToast } from './toast.svelte.js';
	import {
		STUDIO_ENVIRONMENTS,
		STUDIO_SURFACES,
		getStudioPresetMeta,
		hasCustomStudioEnvironment,
		getCustomStudioEnvironmentName,
		subscribeCustomStudioEnvironment,
		setCustomStudioEnvironmentFromFile
	} from '$lib/viewport/studio/studioEnvironment.js';

	/** @type {{ onclose: () => void }} */
	let { onclose } = $props();

	const SECTIONS = [
		{ id: 'general', label: 'General' },
		{ id: 'sketch', label: 'Sketch' },
		{ id: 'appearance', label: 'Appearance' },
		{ id: 'debug', label: 'Debug' },
	];
	let section = $state('general');

	let settings = $derived(getSettings());
	let theme = $derived(getTheme());
	let colorVersion = $derived(getColorVersion());
	let customEnvTick = $state(0);
	/** @type {HTMLInputElement | null} */
	let hdriFileInput = $state(null);

	const unsubCustomEnv = subscribeCustomStudioEnvironment(() => {
		customEnvTick += 1;
	});
	onDestroy(() => unsubCustomEnv());

	let hasCustomEnv = $derived.by(() => {
		void customEnvTick;
		return hasCustomStudioEnvironment();
	});
	let customEnvName = $derived.by(() => {
		void customEnvTick;
		return getCustomStudioEnvironmentName() ?? 'Custom HDRI';
	});
	const hdriPresets = STUDIO_ENVIRONMENTS.filter((e) => e.kind === 'hdri');
	const rigPresets = STUDIO_ENVIRONMENTS.filter((e) => e.kind === 'procedural');

	function handleSceneSelect(e) {
		const nextId = e.currentTarget.value;
		const meta = getStudioPresetMeta(nextId);
		updateSettings({
			studioEnvironment: nextId,
			studioBackdrop: nextId === 'custom' ? true : Boolean(meta?.defaultBackdrop),
			studioSurface: 'auto'
		});
	}

	async function handleCustomHdriUpload(e) {
		const file = e.currentTarget.files?.[0];
		e.currentTarget.value = '';
		if (!file) return;
		try {
			await setCustomStudioEnvironmentFromFile(file);
			updateSettings({
				viewportShading: 'studio',
				studioEnvironment: 'custom',
				studioBackdrop: true
			});
			showToast('success', `Loaded HDRI "${file.name}"`);
		} catch (err) {
			showToast('error', `Could not decode HDRI: ${err?.message ?? err}`);
		}
	}

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
				<label class="row">
					<span class="row-text">
						<span class="row-label">On reload</span>
						<span class="row-desc">What a reloaded tab — or one the browser discarded in the background — does with its last work. Work is kept as a draft in this browser after every change and whenever the tab is hidden.</span>
					</span>
					<select
						class="row-select"
						data-testid="setting-restore-on-reload"
						value={settings.restoreOnReload}
						onchange={(e) => updateSettings({ restoreOnReload: e.currentTarget.value })}
					>
						<option value="auto">Reopen automatically</option>
						<option value="ask">Ask</option>
						<option value="never">Start empty</option>
					</select>
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

			{:else if section === 'debug'}
				<h2>Debug</h2>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-show-rotation-center"
						checked={settings.showRotationCenter}
						onchange={(e) => updateSettings({ showRotationCenter: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Display rotation center</span>
						<span class="row-desc">While you rotate the view, marks the point the model is turning about with a small translucent green sphere. The point is re-anchored to whatever is under the cursor at the start of each rotate, so this shows you what it picked.</span>
					</span>
				</label>

			{:else if section === 'appearance'}
				<h2>Appearance</h2>

				<h3>3D Rendering</h3>
				<label class="row">
					<span class="row-text">
						<span class="row-label">Viewport shading mode</span>
						<span class="row-desc">Classic Shaded uses camera-pinned CAD lights. Studio enables physically based materials, procedural softbox IBL, Ground Truth Ambient &amp; Specular Occlusion, Screen-Space Reflections, and progressive anti-aliased settling.</span>
					</span>
					<select
						class="row-select"
						data-testid="setting-viewport-shading"
						value={settings.viewportShading}
						onchange={(e) => updateSettings({ viewportShading: e.currentTarget.value })}
					>
						<option value="shaded">Classic Shaded</option>
						<option value="studio">Studio PBR</option>
					</select>
				</label>
				<label class="row">
					<span class="row-text">
						<span class="row-label">Studio scene (HDRI / Rig)</span>
						<span class="row-desc">Choose a real-world photographic scene (wooden workbench, walnut office desk, granite countertop, cutting mat, floating outdoor park/plaza), a clean studio rig, or load your own <code>.hdr</code> / <code>.exr</code> file.</span>
					</span>
					<span class="env-controls">
						<select
							class="row-select"
							data-testid="setting-studio-environment"
							value={settings.studioEnvironment === 'custom' && !hasCustomEnv
								? 'studio-softbox'
								: settings.studioEnvironment}
							onchange={handleSceneSelect}
						>
							<optgroup label="Real-World Scenes">
								{#each hdriPresets as env (env.id)}
									<option value={env.id}>{env.label}</option>
								{/each}
								{#if hasCustomEnv}
									<option value="custom">Custom: {customEnvName}</option>
								{/if}
							</optgroup>
							<optgroup label="Clean Studio Rigs">
								{#each rigPresets as env (env.id)}
									<option value={env.id}>{env.label}</option>
								{/each}
							</optgroup>
						</select>
						<button
							type="button"
							class="small-btn"
							data-testid="setting-studio-upload-hdri"
							onclick={() => hdriFileInput?.click()}
						>Load .hdr/.exr…</button>
						<input
							bind:this={hdriFileInput}
							type="file"
							accept=".hdr,.exr"
							style="display:none"
							onchange={handleCustomHdriUpload}
						/>
					</span>
				</label>
				<label class="row">
					<span class="row-text">
						<span class="row-label">Tabletop stage surface</span>
						<span class="row-desc">Places a 3D beveled workbench, walnut desk, polished granite countertop, or cutting mat flush beneath the model's base (with GTAO contact shadows and SSR reflections), or floats the part in air.</span>
					</span>
					<select
						class="row-select"
						data-testid="setting-studio-surface"
						value={settings.studioSurface}
						onchange={(e) => updateSettings({ studioSurface: e.currentTarget.value })}
					>
						{#each STUDIO_SURFACES as surf (surf.id)}
							<option value={surf.id}>{surf.label}</option>
						{/each}
					</select>
				</label>
				<label class="row">
					<span class="row-text">
						<span class="row-label">Environment rotation ({Math.round(settings.studioEnvRotation ?? 0)}°)</span>
						<span class="row-desc">Rotates the HDRI sun, windows, and shop fixtures azimuthally around the vertical Z axis.</span>
					</span>
					<input
						type="range"
						min="0"
						max="360"
						step="5"
						data-testid="setting-studio-env-rotation"
						value={settings.studioEnvRotation ?? 0}
						oninput={(e) => updateSettings({ studioEnvRotation: parseFloat(e.currentTarget.value) })}
					/>
				</label>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-studio-backdrop"
						checked={settings.studioBackdrop}
						onchange={(e) => updateSettings({ studioBackdrop: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Show 4K environment backdrop</span>
						<span class="row-desc">Displays the high-resolution 4096×2048 360° HDRI environment behind the model instead of the flat theme background.</span>
					</span>
				</label>
				<label class="row">
					<span class="row-text">
						<span class="row-label">Backdrop depth-of-field blur ({Math.round((settings.studioBackdropBlur ?? 0) * 100)}%)</span>
						<span class="row-desc">0% keeps the 4K park, workshop, or office photo completely crisp; raise it for shallow-depth-of-field optical bokeh.</span>
					</span>
					<input
						type="range"
						min="0"
						max="1"
						step="0.05"
						data-testid="setting-studio-backdrop-blur"
						value={settings.studioBackdropBlur ?? 0}
						oninput={(e) => updateSettings({ studioBackdropBlur: parseFloat(e.currentTarget.value) })}
					/>
				</label>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-show-edges"
						checked={settings.showModelEdges}
						onchange={(e) => updateSettings({ showModelEdges: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Show B-Rep model edges</span>
						<span class="row-desc">Draws topological wireframe edge lines on 3D solids. Uncheck in Studio mode for clean photorealistic part rendering.</span>
					</span>
				</label>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-show-vertices"
						checked={settings.showModelVertices}
						onchange={(e) => updateSettings({ showModelVertices: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Show B-Rep model vertices</span>
						<span class="row-desc">Draws topological corner vertex dots on 3D solids. Uncheck alongside model edges for clean photorealistic rendering.</span>
					</span>
				</label>
				<label class="row">
					<span class="row-text">
						<span class="row-label">Studio tone mapping</span>
						<span class="row-desc">Khronos PBR Neutral keeps material swatch colors faithful without hue shift; AgX rolls off bright highlights cinematically.</span>
					</span>
					<select
						class="row-select"
						data-testid="setting-studio-tonemapping"
						value={settings.studioToneMapping}
						onchange={(e) => updateSettings({ studioToneMapping: e.currentTarget.value })}
					>
						<option value="neutral">Khronos PBR Neutral</option>
						<option value="agx">AgX Filmic</option>
					</select>
				</label>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-studio-ao"
						checked={settings.studioAO}
						onchange={(e) => updateSettings({ studioAO: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Ground Truth Ambient &amp; Specular Occlusion (GTAO + GTSO)</span>
						<span class="row-desc">Darkens pockets, counterbores, gear roots, and suppresses bright studio reflections inside occluded cavities.</span>
					</span>
				</label>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-studio-ssr"
						checked={settings.studioSSR}
						onchange={(e) => updateSettings({ studioSSR: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Screen-Space Reflections (SSR)</span>
						<span class="row-desc">Reflects neighboring part geometry across shiny metals and clearcoats, falling back smoothly to the studio environment.</span>
					</span>
				</label>
				<label class="row">
					<input
						type="checkbox"
						data-testid="setting-studio-ground-shadow"
						checked={settings.studioGroundShadow}
						onchange={(e) => updateSettings({ studioGroundShadow: e.currentTarget.checked })}
					/>
					<span class="row-text">
						<span class="row-label">Studio ground contact shadow</span>
						<span class="row-desc">Draws a soft grounding shadow plane beneath the model's lowest Z point when viewing from above.</span>
					</span>
				</label>

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
	.row-select {
		margin-left: auto;
		flex-shrink: 0;
		background: var(--bg-tertiary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		padding: 3px 6px;
		font-size: 12px;
	}
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
	.env-controls { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
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
