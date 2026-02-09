<script>
	import { T, useThrelte } from '@threlte/core';
	import * as THREE from 'three';
	import {
		getSketchMode,
		getActiveTool,
		setSketchCursorPos
	} from '$lib/engine/store.svelte.js';
	import { buildSketchPlane, screenToSketchCoords } from './sketchCoords.js';
	import { handleToolEvent, resetTool } from './tools.js';

	const { camera, renderer } = useThrelte();

	let sm = $derived(getSketchMode());
	let activeTool = $derived(getActiveTool());
	let plane = $derived(sm?.active ? buildSketchPlane(sm.origin, sm.normal) : null);

	// Large invisible plane for pointer capture
	const planeGeo = new THREE.PlaneGeometry(200, 200);
	const planeMat = new THREE.MeshBasicMaterial({
		visible: false,
		side: THREE.DoubleSide
	});

	/**
	 * Compute sketch units per screen pixel for dynamic thresholds.
	 */
	function getScreenPixelSize() {
		if (!$camera || !plane) return 0.01;
		const cam = $camera;
		if (cam instanceof THREE.PerspectiveCamera) {
			const dist = cam.position.distanceTo(plane.origin);
			const vFov = cam.fov * (Math.PI / 180);
			const heightAtDist = 2 * dist * Math.tan(vFov / 2);
			return heightAtDist / ($renderer?.domElement?.clientHeight || 800);
		}
		// Orthographic
		if (cam instanceof THREE.OrthographicCamera) {
			const height = cam.top - cam.bottom;
			return height / ($renderer?.domElement?.clientHeight || 800);
		}
		return 0.01;
	}

	/**
	 * Convert a Threlte pointer event to sketch coordinates and delegate to tool.
	 */
	function onPointerEvent(eventType, event) {
		if (!plane || !$camera || !$renderer?.domElement) return;

		const domEl = $renderer.domElement;
		const nativeEvent = event.nativeEvent || event;

		const coords = screenToSketchCoords(nativeEvent, domEl, $camera, plane);
		if (!coords) return;

		const screenPixelSize = getScreenPixelSize();
		const shiftKey = nativeEvent.shiftKey ?? false;

		if (eventType === 'pointermove') {
			setSketchCursorPos({ x: coords.x, y: coords.y });
		}

		handleToolEvent(activeTool, eventType, coords.x, coords.y, screenPixelSize, shiftKey);
	}

	// Reset tool state when switching tools
	$effect(() => {
		// Access activeTool to establish dependency
		const _ = activeTool;
		resetTool();
	});
</script>

{#if sm?.active && plane}
	<T.Mesh
		geometry={planeGeo}
		material={planeMat}
		position={[plane.origin.x, plane.origin.y, plane.origin.z]}
		quaternion={plane.quaternion}
		renderOrder={-1}
		onpointerdown={(e) => onPointerEvent('pointerdown', e)}
		onpointermove={(e) => onPointerEvent('pointermove', e)}
	/>
{/if}
