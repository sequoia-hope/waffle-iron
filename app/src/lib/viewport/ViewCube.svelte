<script>
	import { onDestroy } from 'svelte';
	import {
		getViewCubeTransform, getCameraProjection, toggleCameraProjection
	} from '$lib/engine/store.svelte.js';
	import { getSettings, updateSettings } from '$lib/ui/settings.svelte.js';
	import {
		STUDIO_ENVIRONMENTS,
		STUDIO_SURFACES,
		getStudioPresetMeta,
		hasCustomStudioEnvironment,
		getCustomStudioEnvironmentName,
		subscribeCustomStudioEnvironment
	} from './studio/studioEnvironment.js';

	let settings = $derived(getSettings());
	let cubeTransform = $derived(getViewCubeTransform());
	let isOrtho = $derived(getCameraProjection() === 'orthographic');
	let isStudio = $derived(settings.viewportShading === 'studio');
	let dropdownOpen = $state(false);
	let currentView = $state('iso');
	let customTick = $state(0);

	const unsubCustom = subscribeCustomStudioEnvironment(() => {
		customTick += 1;
	});
	onDestroy(() => unsubCustom());

	let hasCustomEnv = $derived.by(() => {
		void customTick;
		return hasCustomStudioEnvironment();
	});
	let customEnvLabel = $derived.by(() => {
		void customTick;
		return getCustomStudioEnvironmentName() ?? 'Custom HDRI';
	});
	const hdriPresets = STUDIO_ENVIRONMENTS.filter((e) => e.kind === 'hdri');
	const rigPresets = STUDIO_ENVIRONMENTS.filter((e) => e.kind === 'procedural');

	// Drag-to-orbit state
	let dragging = $state(false);
	let dragStartX = 0;
	let dragStartY = 0;
	let pointerDownX = 0;
	let pointerDownY = 0;
	let didDrag = false;
	const DRAG_THRESHOLD = 4;

	/**
	 * Dispatch a snap-view-and-fit event for the given view name.
	 * @param {string} name
	 */
	function snapToView(name) {
		currentView = name;
		window.dispatchEvent(new CustomEvent('waffle-snap-view-and-fit', { detail: { view: name } }));
	}

	function handleFitAll() {
		window.dispatchEvent(new Event('waffle-fit-all'));
		dropdownOpen = false;
	}

	function handleToggleProjection() {
		toggleCameraProjection();
		dropdownOpen = false;
	}

	function handleToggleStudio() {
		updateSettings({ viewportShading: isStudio ? 'shaded' : 'studio' });
	}

	function handleSceneSelect(e) {
		const nextId = e.currentTarget.value;
		const meta = getStudioPresetMeta(nextId);
		updateSettings({
			studioEnvironment: nextId,
			studioBackdrop: nextId === 'custom' ? true : Boolean(meta?.defaultBackdrop),
			studioSurface: 'auto'
		});
	}

	function toggleDropdown() {
		dropdownOpen = !dropdownOpen;
	}

	function closeDropdown() {
		dropdownOpen = false;
	}

	/** @type {HTMLElement|null} */
	let cubeSceneEl = null;

	function handlePointerDown(e) {
		if (e.button !== 0) return;
		dragging = true;
		didDrag = false;
		dragStartX = e.clientX;
		dragStartY = e.clientY;
		pointerDownX = e.clientX;
		pointerDownY = e.clientY;
		e.currentTarget.setPointerCapture(e.pointerId);
		e.preventDefault();
	}

	function handlePointerMove(e) {
		if (!dragging) return;
		const dx = e.clientX - dragStartX;
		const dy = e.clientY - dragStartY;
		dragStartX = e.clientX;
		dragStartY = e.clientY;

		const totalDx = e.clientX - pointerDownX;
		const totalDy = e.clientY - pointerDownY;
		if (Math.abs(totalDx) > DRAG_THRESHOLD || Math.abs(totalDy) > DRAG_THRESHOLD) {
			didDrag = true;
		}

		if (didDrag) {
			window.dispatchEvent(new CustomEvent('waffle-viewcube-orbit', {
				detail: { dx, dy }
			}));
		}
	}

	function handlePointerUp(e) {
		if (!dragging) return;
		dragging = false;
		if (!didDrag) {
			// Pointer capture redirects target to cube-scene, so use elementFromPoint
			// to find the actual face under the cursor
			const el = document.elementFromPoint(pointerDownX, pointerDownY);
			const face = el?.closest?.('[data-view]');
			if (face) {
				snapToView(face.dataset.view);
			}
		}
	}
</script>

<svelte:window onclick={closeDropdown} />

<div class="viewcube-container" data-testid="viewcube-overlay">
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="cube-scene"
		class:ortho={isOrtho}
		class:dragging
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
		onpointercancel={handlePointerUp}
	>
		<div class="cube" style:transform={cubeTransform}>
			{#each ['front', 'back', 'top', 'bottom', 'left', 'right'] as view}
				<div
					class="face {view}"
					class:active={currentView === view}
					data-testid="viewcube-btn-{view}"
					data-view={view}
				>{view.toUpperCase()}</div>
			{/each}
		</div>
	</div>
	<div class="cube-controls">
		<button class="iso-btn" class:active={currentView === 'iso'} data-testid="viewcube-btn-iso" onclick={() => snapToView('iso')}>ISO</button>
		<button class="dropdown-toggle" onclick={(e) => { e.stopPropagation(); toggleDropdown(); }}>&#x25BC;</button>
	</div>
	{#if dropdownOpen}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="dropdown-panel" onclick={(e) => e.stopPropagation()} data-testid="viewcube-dropdown">
			<button class="dropdown-item" onclick={handleFitAll}>Fit All (F)</button>
			<div class="dropdown-sep"></div>
			<label class="dropdown-label">
				<input type="checkbox" checked={isOrtho} onchange={handleToggleProjection} data-testid="viewcube-ortho-toggle" />
				Ortho
			</label>
			<label class="dropdown-label" title="Show B-Rep wireframe edges on 3D bodies">
				<input
					type="checkbox"
					checked={settings.showModelEdges}
					onchange={(e) => updateSettings({ showModelEdges: e.currentTarget.checked })}
					data-testid="viewcube-edges-toggle"
				/>
				Edges
			</label>
			<label class="dropdown-label" title="Show B-Rep vertex dots on 3D bodies">
				<input
					type="checkbox"
					checked={settings.showModelVertices}
					onchange={(e) => updateSettings({ showModelVertices: e.currentTarget.checked })}
					data-testid="viewcube-vertices-toggle"
				/>
				Vertices
			</label>
			<div class="dropdown-sep"></div>
			<label class="dropdown-label" title="Physically based Studio lighting (HDRI / PMREM + GTAO + GTSO + SSR)">
				<input type="checkbox" checked={isStudio} onchange={handleToggleStudio} data-testid="viewcube-studio-toggle" />
				Studio
			</label>
			{#if isStudio}
				<div class="dropdown-sub">
					<select
						class="dropdown-select"
						data-testid="viewcube-studio-env"
						aria-label="Studio scene"
						title="Studio scene (lighting + backdrop + tabletop)"
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
								<option value="custom">Custom: {customEnvLabel}</option>
							{/if}
						</optgroup>
						<optgroup label="Clean Studio Rigs">
							{#each rigPresets as env (env.id)}
								<option value={env.id}>{env.label}</option>
							{/each}
						</optgroup>
					</select>
				</div>
				<div class="dropdown-sub">
					<select
						class="dropdown-select"
						data-testid="viewcube-studio-surface"
						aria-label="Tabletop surface"
						title="Physical tabletop surface beneath the model"
						value={settings.studioSurface}
						onchange={(e) => updateSettings({ studioSurface: e.currentTarget.value })}
					>
						{#each STUDIO_SURFACES as surf (surf.id)}
							<option value={surf.id}>Surface: {surf.label}</option>
						{/each}
					</select>
				</div>
				<label class="dropdown-label" title="Show crisp 4K 360° HDRI environment behind the part">
					<input
						type="checkbox"
						checked={settings.studioBackdrop}
						onchange={(e) => updateSettings({ studioBackdrop: e.currentTarget.checked })}
						data-testid="viewcube-studio-backdrop"
					/>
					Backdrop
				</label>
				<label class="dropdown-label" title="Preview exterior FDM 3D-printed filament layer lines along Z">
					<input
						type="checkbox"
						checked={settings.studioPrintLayers}
						onchange={(e) => updateSettings({ studioPrintLayers: e.currentTarget.checked })}
						data-testid="viewcube-print-layers-toggle"
					/>
					3D Print Layers
				</label>
			{/if}
		</div>
	{/if}
</div>

<style>
	.viewcube-container {
		position: absolute;
		top: 8px;
		right: max(8px, env(safe-area-inset-right, 0px));
		z-index: 10;
		pointer-events: auto;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}

	.cube-scene {
		width: 60px;
		height: 60px;
		perspective: 200px;
		cursor: grab;
	}

	.cube-scene.ortho {
		perspective: none;
	}

	.cube-scene.dragging {
		cursor: grabbing;
	}

	.cube {
		width: 60px;
		height: 60px;
		position: relative;
		transform-style: preserve-3d;
		transition: transform 0.05s linear;
	}

	.face {
		position: absolute;
		width: 60px;
		height: 60px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in srgb, var(--bg-secondary) 85%, transparent);
		border: 1px solid var(--border-color);
		color: var(--text-secondary);
		font-size: 8px;
		font-weight: 700;
		letter-spacing: 0.5px;
		cursor: pointer;
		backface-visibility: hidden;
		padding: 0;
		font-family: inherit;
	}

	/* Opaque on hover: a semi-transparent accent over an arbitrary viewport
	   background cannot be relied on to carry --text-on-accent. */
	.face:hover {
		background: var(--accent);
		color: var(--text-on-accent);
		border-color: var(--accent);
	}

	.face.active {
		color: var(--accent);
		border-color: var(--accent);
	}

	/* The cube is drawn in WORLD axes (its transform is the inverted camera
	   quaternion), so each label sits on the world face that view looks at —
	   and the world is Z-up (`standardViews` in CameraControls). The trailing
	   rotateZ rolls a face about its own normal so its text reads upright from
	   that view's camera. */
	.top    { transform: rotateY(0deg) translateZ(30px); }                    /* +Z */
	.bottom { transform: rotateY(180deg) rotateZ(180deg) translateZ(30px); }  /* −Z */
	.front  { transform: rotateX(90deg) translateZ(30px); }                   /* −Y */
	.back   { transform: rotateX(-90deg) rotateZ(180deg) translateZ(30px); }  /* +Y */
	.left   { transform: rotateY(-90deg) rotateZ(-90deg) translateZ(30px); } /* −X */
	.right  { transform: rotateY(90deg) rotateZ(90deg) translateZ(30px); }   /* +X */

	.cube-controls {
		display: flex;
		gap: 2px;
		background: color-mix(in srgb, var(--bg-secondary) 70%, transparent);
		border-radius: 4px;
		padding: 2px;
		backdrop-filter: blur(4px);
	}

	.iso-btn {
		background: transparent;
		border: none;
		color: var(--text-secondary);
		font-size: 10px;
		font-weight: 600;
		padding: 3px 8px;
		cursor: pointer;
		border-radius: 2px;
		font-family: inherit;
	}

	.iso-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.iso-btn.active {
		color: var(--accent);
	}

	.dropdown-toggle {
		background: transparent;
		border: none;
		color: var(--text-muted);
		font-size: 8px;
		padding: 3px 6px;
		cursor: pointer;
		border-radius: 2px;
		font-family: inherit;
	}

	.dropdown-toggle:hover {
		background: var(--bg-hover);
		color: var(--text-secondary);
	}

	.dropdown-panel {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		min-width: 172px;
		max-height: calc(100vh - 110px - env(safe-area-inset-bottom, 0px));
		overflow-y: auto;
		background: color-mix(in srgb, var(--bg-tertiary) 92%, transparent);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		padding: 4px 0;
		backdrop-filter: blur(8px);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		white-space: nowrap;
	}

	.dropdown-sub {
		padding: 3px 10px 4px;
	}

	.dropdown-select {
		width: 100%;
		background: var(--bg-secondary);
		color: var(--text-primary);
		border: 1px solid var(--border-color);
		border-radius: 4px;
		font-size: 11px;
		font-family: inherit;
		padding: 3px 6px;
		cursor: pointer;
	}

	.dropdown-item {
		display: block;
		width: 100%;
		background: transparent;
		border: none;
		color: var(--text-primary);
		font-size: 11px;
		padding: 6px 12px;
		cursor: pointer;
		text-align: left;
		font-family: inherit;
	}

	.dropdown-item:hover {
		background: var(--accent);
		color: var(--text-on-accent);
	}

	.dropdown-sep {
		height: 1px;
		background: var(--border-color);
		margin: 2px 0;
	}

	.dropdown-label {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-primary);
		font-size: 11px;
		padding: 6px 12px;
		cursor: pointer;
	}

	.dropdown-label:hover {
		background: var(--accent);
		color: var(--text-on-accent);
	}

	.dropdown-label input[type="checkbox"] {
		accent-color: var(--accent);
	}
</style>
