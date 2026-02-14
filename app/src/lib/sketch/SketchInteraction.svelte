<script>
	import { useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import {
		getSketchMode,
		getActiveTool,
		setSketchCursorPos
	} from '$lib/engine/store.svelte.js';
	import { buildSketchPlane, screenToSketchCoords } from './sketchCoords.js';
	import { handleToolEvent, resetTool } from './tools.js';

	const { camera, renderer } = useThrelte();

	// renderer is a plain THREE.WebGLRenderer, renderer.domElement is the canvas
	// camera is a Svelte store — use camera.current to get the THREE.Camera

	let activeTool = $derived(getActiveTool());

	/**
	 * Compute sketch units per screen pixel for dynamic thresholds.
	 */
	function getScreenPixelSize(cam, planeOrigin, canvasHeight) {
		if (!cam) return 0.01;
		if (cam instanceof THREE.PerspectiveCamera) {
			const dist = cam.position.distanceTo(planeOrigin);
			const vFov = cam.fov * (Math.PI / 180);
			const heightAtDist = 2 * dist * Math.tan(vFov / 2);
			return heightAtDist / (canvasHeight || 800);
		}
		if (cam instanceof THREE.OrthographicCamera) {
			const height = cam.top - cam.bottom;
			return height / (canvasHeight || 800);
		}
		return 0.01;
	}

	onMount(() => {
		const canvas = renderer.domElement;
		if (!canvas) return;

		/** @param {PointerEvent} e */
		function handler(e) {
			// Skip secondary pointers (e.g. second finger in multi-touch)
			if (!e.isPrimary) return;
			// Skip right/middle mouse button on pointerdown
			if (e.pointerType === 'mouse' && e.button !== 0 && e.type === 'pointerdown') return;

			const sm = getSketchMode();
			if (!sm?.active) return;

			const cam = camera.current;
			if (!cam) return;

			const plane = buildSketchPlane(sm.origin, sm.normal);
			if (!plane) return;

			const coords = screenToSketchCoords(e, canvas, cam, plane);
			if (!coords) return;

			const screenPixelSize = getScreenPixelSize(cam, plane.origin, canvas.clientHeight);
			const shiftKey = e.shiftKey;
			const tool = getActiveTool();

			if (e.type === 'pointermove') {
				setSketchCursorPos({ x: coords.x, y: coords.y });
			}

			handleToolEvent(tool, e.type, coords.x, coords.y, screenPixelSize, shiftKey);
		}

		// pointerdown on canvas; pointermove/pointerup on window because
		// OrbitControls calls setPointerCapture() which redirects pointer
		// events away from the canvas during drags.
		canvas.addEventListener('pointerdown', handler);
		window.addEventListener('pointermove', handler);
		window.addEventListener('pointerup', handler);

		return () => {
			canvas.removeEventListener('pointerdown', handler);
			window.removeEventListener('pointermove', handler);
			window.removeEventListener('pointerup', handler);
		};
	});

	// Reset tool state when switching tools
	$effect(() => {
		const _ = activeTool;
		resetTool();
	});
</script>

<!-- Minimal template for Svelte 5 lifecycle -->
{#if false}{/if}
