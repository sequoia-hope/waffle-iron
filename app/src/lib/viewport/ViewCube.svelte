<script>
	import {
		getViewCubeTransform, getCameraProjection, toggleCameraProjection
	} from '$lib/engine/store.svelte.js';

	let cubeTransform = $derived(getViewCubeTransform());
	let isOrtho = $derived(getCameraProjection() === 'orthographic');
	let dropdownOpen = $state(false);
	let currentView = $state('iso');

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

	.front  { transform: rotateY(0deg) translateZ(30px); }
	.back   { transform: rotateY(180deg) translateZ(30px); }
	.top    { transform: rotateX(90deg) translateZ(30px); }
	.bottom { transform: rotateX(-90deg) translateZ(30px); }
	.left   { transform: rotateY(-90deg) translateZ(30px); }
	.right  { transform: rotateY(90deg) translateZ(30px); }

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
		background: color-mix(in srgb, var(--bg-tertiary) 92%, transparent);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		padding: 4px 0;
		backdrop-filter: blur(8px);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		white-space: nowrap;
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
