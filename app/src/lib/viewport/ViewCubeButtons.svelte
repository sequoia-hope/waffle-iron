<script>
	let currentView = $state('iso');

	/**
	 * Dispatch a custom event that CameraControls listens for.
	 * @param {string} name
	 */
	function snapToView(name) {
		currentView = name;
		window.dispatchEvent(new CustomEvent('waffle-snap-view', { detail: { view: name } }));
	}

	const views = ['front', 'back', 'top', 'bottom', 'left', 'right', 'iso'];
</script>

<div class="viewcube-overlay">
	<div class="viewcube-buttons">
		{#each views as name}
			<button
				class="view-btn"
				class:active={currentView === name}
				onclick={() => snapToView(name)}
			>
				{name.charAt(0).toUpperCase() + name.slice(1)}
			</button>
		{/each}
	</div>
</div>

<style>
	.viewcube-overlay {
		position: absolute;
		bottom: 8px;
		right: 8px;
		pointer-events: auto;
		z-index: 10;
	}

	.viewcube-buttons {
		display: flex;
		flex-direction: column;
		gap: 1px;
		background: rgba(30, 30, 30, 0.7);
		border-radius: 4px;
		padding: 3px;
		backdrop-filter: blur(4px);
	}

	.view-btn {
		background: transparent;
		border: none;
		color: #999;
		font-size: 10px;
		padding: 2px 6px;
		cursor: pointer;
		border-radius: 2px;
		text-align: left;
	}

	.view-btn:hover {
		background: rgba(255, 255, 255, 0.1);
		color: #ccc;
	}

	.view-btn.active {
		color: #4488ff;
	}
</style>
