<script>
	import {
		getViewCubeTransform, getCameraProjection, toggleCameraProjection
	} from '$lib/engine/store.svelte.js';

	let cubeTransform = $derived(getViewCubeTransform());
	let isOrtho = $derived(getCameraProjection() === 'orthographic');
	let dropdownOpen = $state(false);
	let currentView = $state('iso');

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
</script>

<svelte:window onclick={closeDropdown} />

<div class="viewcube-container" data-testid="viewcube-overlay">
	<div class="cube-scene">
		<div class="cube" style:transform={cubeTransform}>
			<button class="face front" class:active={currentView === 'front'} data-testid="viewcube-btn-front" onclick={() => snapToView('front')}>FRONT</button>
			<button class="face back" class:active={currentView === 'back'} data-testid="viewcube-btn-back" onclick={() => snapToView('back')}>BACK</button>
			<button class="face top" class:active={currentView === 'top'} data-testid="viewcube-btn-top" onclick={() => snapToView('top')}>TOP</button>
			<button class="face bottom" class:active={currentView === 'bottom'} data-testid="viewcube-btn-bottom" onclick={() => snapToView('bottom')}>BOTTOM</button>
			<button class="face left" class:active={currentView === 'left'} data-testid="viewcube-btn-left" onclick={() => snapToView('left')}>LEFT</button>
			<button class="face right" class:active={currentView === 'right'} data-testid="viewcube-btn-right" onclick={() => snapToView('right')}>RIGHT</button>
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
				Orthographic
			</label>
		</div>
	{/if}
</div>

<style>
	.viewcube-container {
		position: absolute;
		top: 8px;
		right: 8px;
		z-index: 10;
		pointer-events: auto;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}

	.cube-scene {
		width: 80px;
		height: 80px;
		perspective: 300px;
	}

	.cube {
		width: 80px;
		height: 80px;
		position: relative;
		transform-style: preserve-3d;
		transition: transform 0.05s linear;
	}

	.face {
		position: absolute;
		width: 80px;
		height: 80px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(45, 45, 55, 0.85);
		border: 1px solid rgba(100, 100, 120, 0.4);
		color: rgba(200, 200, 210, 0.9);
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.5px;
		cursor: pointer;
		backface-visibility: hidden;
		padding: 0;
		font-family: inherit;
	}

	.face:hover {
		background: rgba(0, 120, 212, 0.5);
		color: white;
		border-color: rgba(0, 120, 212, 0.7);
	}

	.face.active {
		color: var(--accent, #4488ff);
		border-color: rgba(0, 120, 212, 0.5);
	}

	.front  { transform: rotateY(0deg) translateZ(40px); }
	.back   { transform: rotateY(180deg) translateZ(40px); }
	.top    { transform: rotateX(90deg) translateZ(40px); }
	.bottom { transform: rotateX(-90deg) translateZ(40px); }
	.left   { transform: rotateY(-90deg) translateZ(40px); }
	.right  { transform: rotateY(90deg) translateZ(40px); }

	.cube-controls {
		display: flex;
		gap: 2px;
		background: rgba(30, 30, 40, 0.7);
		border-radius: 4px;
		padding: 2px;
		backdrop-filter: blur(4px);
	}

	.iso-btn {
		background: transparent;
		border: none;
		color: #999;
		font-size: 10px;
		font-weight: 600;
		padding: 3px 8px;
		cursor: pointer;
		border-radius: 2px;
		font-family: inherit;
	}

	.iso-btn:hover {
		background: rgba(255, 255, 255, 0.1);
		color: #ccc;
	}

	.iso-btn.active {
		color: var(--accent, #4488ff);
	}

	.dropdown-toggle {
		background: transparent;
		border: none;
		color: #777;
		font-size: 8px;
		padding: 3px 6px;
		cursor: pointer;
		border-radius: 2px;
		font-family: inherit;
	}

	.dropdown-toggle:hover {
		background: rgba(255, 255, 255, 0.1);
		color: #aaa;
	}

	.dropdown-panel {
		background: rgba(30, 30, 40, 0.9);
		border: 1px solid rgba(100, 100, 120, 0.3);
		border-radius: 6px;
		padding: 4px 0;
		backdrop-filter: blur(8px);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		min-width: 140px;
	}

	.dropdown-item {
		display: block;
		width: 100%;
		background: transparent;
		border: none;
		color: #ccc;
		font-size: 11px;
		padding: 6px 12px;
		cursor: pointer;
		text-align: left;
		font-family: inherit;
	}

	.dropdown-item:hover {
		background: rgba(0, 120, 212, 0.3);
		color: white;
	}

	.dropdown-sep {
		height: 1px;
		background: rgba(100, 100, 120, 0.3);
		margin: 2px 0;
	}

	.dropdown-label {
		display: flex;
		align-items: center;
		gap: 6px;
		color: #ccc;
		font-size: 11px;
		padding: 6px 12px;
		cursor: pointer;
	}

	.dropdown-label:hover {
		background: rgba(0, 120, 212, 0.3);
		color: white;
	}

	.dropdown-label input[type="checkbox"] {
		accent-color: var(--accent, #0078d4);
	}
</style>
