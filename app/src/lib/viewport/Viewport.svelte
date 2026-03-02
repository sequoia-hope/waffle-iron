<script>
	import { Canvas } from '@threlte/core';
	import Scene from './Scene.svelte';
	import ViewCubeGizmo from './ViewCubeGizmo.svelte';
	import ViewCubeButtons from './ViewCubeButtons.svelte';
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

	let constraintMenuPos = $state({ x: 0, y: 0 });
	let constraintMenuVisible = $state(false);
	let ctxMenuPos = $state({ x: 0, y: 0 });
	let ctxMenuVisible = $state(false);

	/**
	 * Right-click disambiguation: orbit drag vs context menu.
	 * In Chromium, contextmenu fires BEFORE pointerup on right-click,
	 * so we always suppress it and show the menu on pointerup instead.
	 */
	let rightDownPos = null;
	const RIGHT_DRAG_THRESHOLD = 5;

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
		// Always suppress native contextmenu; we show our own on pointerup
		e.preventDefault();
	}
</script>

<div class="viewport" data-testid="viewport" bind:this={viewportEl} oncontextmenu={handleContextMenu}>
	<Canvas>
		<Scene />
		<ViewCubeGizmo />
	</Canvas>
	<ViewCubeButtons />
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
</div>

<style>
	.viewport {
		width: 100%;
		height: 100%;
		background: #1a1a2e;
		position: relative;
		touch-action: none;
	}
</style>
