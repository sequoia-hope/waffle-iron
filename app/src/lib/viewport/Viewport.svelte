<script>
	import { Canvas } from '@threlte/core';
	import { WebGLRenderer } from 'three';
	import Scene from './Scene.svelte';
	import ViewCube from './ViewCube.svelte';
	import ConstraintMenu from '$lib/sketch/ConstraintMenu.svelte';
	import DimensionInput from '$lib/sketch/DimensionInput.svelte';
	import ViewportContextMenu from './ViewportContextMenu.svelte';
	import ExtrudeDialog from '$lib/ui/ExtrudeDialog.svelte';
	import ChamferDialog from '$lib/ui/ChamferDialog.svelte';
	import FilletDialog from '$lib/ui/FilletDialog.svelte';
	import ShellDialog from '$lib/ui/ShellDialog.svelte';
	import BooleanDialog from '$lib/ui/BooleanDialog.svelte';
	import AutoRestoreDialog from '$lib/ui/AutoRestoreDialog.svelte';
	import SketchPlanePrompt from '$lib/ui/SketchPlanePrompt.svelte';
	import GearDialog from '$lib/ui/GearDialog.svelte';
	import { isRebuilding, getSketchMode } from '$lib/engine/store.svelte.js';
	import { longPressContextMenu } from '$lib/ui/longPressContextMenu.js';

	const createRenderer = (canvas) => new WebGLRenderer({
		canvas,
		powerPreference: 'high-performance',
		antialias: true,
		alpha: true,
		// Stencil buffer required by the capped section view (SectionCap.svelte).
		// three.js r163+ defaults `stencil` to false; without it the cap's
		// stencil test always passes and the cap quad fills the whole viewport.
		stencil: true,
		logarithmicDepthBuffer: true
	});

	let showSpinner = $derived(isRebuilding());

	let constraintMenuPos = $state({ x: 0, y: 0 });
	let constraintMenuVisible = $state(false);
	let ctxMenuPos = $state({ x: 0, y: 0 });
	let ctxMenuVisible = $state(false);

	/**
	 * Right-click disambiguation: orbit drag vs context menu.
	 * In Chromium, contextmenu fires BEFORE pointerup on right-click,
	 * so we always suppress it and show the menu on pointerup instead.
	 */
	import { RIGHT_DRAG_THRESHOLD } from '$lib/config.js';
	let rightDownPos = null;

	let viewportEl = $state(null);

	$effect(() => {
		if (!viewportEl) return;

		function onPointerDown(e) {
			if (e.button === 2) {
				rightDownPos = { x: e.clientX, y: e.clientY };
			}
		}

		function onPointerUp(e) {
			if (e.button === 2 && rightDownPos) {
				const dx = e.clientX - rightDownPos.x;
				const dy = e.clientY - rightDownPos.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				const downPos = rightDownPos;
				rightDownPos = null;

				// Only show context menu for stationary right-clicks
				if (dist <= RIGHT_DRAG_THRESHOLD) {
					ctxMenuPos = { x: downPos.x, y: downPos.y };
					ctxMenuVisible = true;
				}
			}
		}

		viewportEl.addEventListener('pointerdown', onPointerDown);
		window.addEventListener('pointerup', onPointerUp);
		return () => {
			viewportEl.removeEventListener('pointerdown', onPointerDown);
			window.removeEventListener('pointerup', onPointerUp);
		};
	});

	function handleContextMenu(e) {
		e.preventDefault();
		// Only show viewport menu for synthetic contextmenu events (from longPressContextMenu).
		// Native browser contextmenu events are just suppressed — on desktop, the pointerup
		// handler above handles right-click menu display.
		// Skip if sketch mode is active — let ConstraintMenu handle it.
		if (!e.isTrusted) {
			if (getSketchMode()?.active) return;
			ctxMenuPos = { x: e.clientX, y: e.clientY };
			ctxMenuVisible = true;
		}
	}
</script>

<div class="viewport" data-testid="viewport" bind:this={viewportEl} oncontextmenu={handleContextMenu} use:longPressContextMenu>
	<Canvas {createRenderer}>
		<Scene />
	</Canvas>
	<ViewCube />
	<ConstraintMenu bind:menuPos={constraintMenuPos} bind:visible={constraintMenuVisible} />
	<DimensionInput />
	<ViewportContextMenu bind:pos={ctxMenuPos} bind:visible={ctxMenuVisible} />
	<ExtrudeDialog />
	<ChamferDialog />
	<FilletDialog />
	<ShellDialog />
	<BooleanDialog />
	<AutoRestoreDialog />
	<SketchPlanePrompt />
	<GearDialog />
	{#if showSpinner}
		<div class="rebuild-overlay" data-testid="rebuild-spinner">
			<div class="rebuild-spinner"></div>
			<span class="rebuild-label">Rebuilding...</span>
		</div>
	{/if}
</div>

<style>
	.viewport {
		width: 100%;
		height: 100%;
		background: #1a1a2e;
		position: relative;
		touch-action: none;
	}

	.rebuild-overlay {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		background: rgba(0, 0, 0, 0.35);
		z-index: 50;
		pointer-events: none;
	}

	.rebuild-spinner {
		width: 32px;
		height: 32px;
		border: 3px solid rgba(255, 255, 255, 0.2);
		border-top-color: rgba(255, 255, 255, 0.8);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	.rebuild-label {
		margin-top: 8px;
		color: rgba(255, 255, 255, 0.7);
		font-size: 12px;
		font-family: inherit;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
