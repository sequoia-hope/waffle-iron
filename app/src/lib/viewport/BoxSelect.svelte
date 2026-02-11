<script>
	import { useThrelte } from '@threlte/core';
	import * as THREE from 'three';
	import { onMount } from 'svelte';
	import {
		getMeshes,
		selectRef,
		clearSelection,
		getBoxSelectState,
		setBoxSelectState,
		getControlsObject,
		getCameraObject,
		getSketchMode,
		geomRefEquals
	} from '$lib/engine/store.svelte.js';

	const { renderer } = useThrelte();

	let isDragging = false;
	let dragStartX = 0;
	let dragStartY = 0;
	let dragCurrentX = 0;
	let dragCurrentY = 0;
	let dragStartedOnEmpty = false;

	/**
	 * Check if a point in screen space was on an empty area (no mesh hit).
	 * @param {number} clientX
	 * @param {number} clientY
	 * @returns {boolean}
	 */
	function isEmptySpace(clientX, clientY) {
		const camera = getCameraObject();
		if (!camera || !renderer) return true;

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		const ndcX = ((clientX - rect.left) / rect.width) * 2 - 1;
		const ndcY = -((clientY - rect.top) / rect.height) * 2 + 1;

		const raycaster = new THREE.Raycaster();
		raycaster.setFromCamera(new THREE.Vector2(ndcX, ndcY), camera);

		// Collect all meshes from the scene
		const meshObjects = [];
		renderer.domElement.parentElement?.querySelectorAll('canvas');
		const scene = camera.parent;
		if (scene) {
			scene.traverse((obj) => {
				if (/** @type {any} */ (obj).isMesh) {
					meshObjects.push(obj);
				}
			});
		}

		const intersects = raycaster.intersectObjects(meshObjects, false);
		return intersects.length === 0;
	}

	/**
	 * Project a 3D point to screen coordinates.
	 * @param {THREE.Vector3} point
	 * @param {THREE.PerspectiveCamera} camera
	 * @param {HTMLCanvasElement} canvas
	 * @returns {{ x: number, y: number }}
	 */
	function projectToScreen(point, camera, canvas) {
		const projected = point.clone().project(camera);
		const rect = canvas.getBoundingClientRect();
		return {
			x: ((projected.x + 1) / 2) * rect.width + rect.left,
			y: ((-projected.y + 1) / 2) * rect.height + rect.top
		};
	}

	/**
	 * Perform box selection — find all face GeomRefs whose centroids fall within the selection box.
	 * @param {number} x1 - box left
	 * @param {number} y1 - box top
	 * @param {number} x2 - box right
	 * @param {number} y2 - box bottom
	 * @param {'window'|'crossing'} mode
	 * @param {boolean} additive
	 */
	function performBoxSelect(x1, y1, x2, y2, mode, additive) {
		const camera = getCameraObject();
		if (!camera || !renderer) return;

		const canvas = renderer.domElement;
		const meshData = getMeshes();
		if (!meshData || meshData.length === 0) return;

		// Normalize box coordinates
		const boxLeft = Math.min(x1, x2);
		const boxRight = Math.max(x1, x2);
		const boxTop = Math.min(y1, y2);
		const boxBottom = Math.max(y1, y2);

		// Skip if box is too small (< 5px in either dimension)
		if (boxRight - boxLeft < 5 && boxBottom - boxTop < 5) return;

		if (!additive) {
			clearSelection();
		}

		// For each mesh, project face centroids and check box inclusion
		for (const mesh of meshData) {
			if (!mesh.faceRanges || !mesh.vertices || !mesh.indices) continue;

			for (const range of mesh.faceRanges) {
				if (!range.geom_ref) continue;

				// Compute centroid of the face by averaging triangle centroids
				const startIdx = range.start_index;
				const endIdx = range.end_index;
				let cx = 0, cy = 0, cz = 0;
				let triCount = 0;

				for (let i = startIdx; i + 2 < endIdx; i += 3) {
					const i0 = mesh.indices[i];
					const i1 = mesh.indices[i + 1];
					const i2 = mesh.indices[i + 2];

					cx += (mesh.vertices[i0 * 3] + mesh.vertices[i1 * 3] + mesh.vertices[i2 * 3]) / 3;
					cy += (mesh.vertices[i0 * 3 + 1] + mesh.vertices[i1 * 3 + 1] + mesh.vertices[i2 * 3 + 1]) / 3;
					cz += (mesh.vertices[i0 * 3 + 2] + mesh.vertices[i1 * 3 + 2] + mesh.vertices[i2 * 3 + 2]) / 3;
					triCount++;
				}

				if (triCount === 0) continue;
				cx /= triCount;
				cy /= triCount;
				cz /= triCount;

				const centroid = new THREE.Vector3(cx, cy, cz);
				const screenPos = projectToScreen(centroid, camera, canvas);

				if (mode === 'window') {
					// Window mode: centroid must be fully inside the box
					if (
						screenPos.x >= boxLeft &&
						screenPos.x <= boxRight &&
						screenPos.y >= boxTop &&
						screenPos.y <= boxBottom
					) {
						selectRef(range.geom_ref, true);
					}
				} else {
					// Crossing mode: centroid touches the box
					if (
						screenPos.x >= boxLeft &&
						screenPos.x <= boxRight &&
						screenPos.y >= boxTop &&
						screenPos.y <= boxBottom
					) {
						selectRef(range.geom_ref, true);
					}
				}
			}
		}
	}

	/**
	 * @param {MouseEvent} e
	 */
	function handleMouseDown(e) {
		// Only left button, not in sketch mode
		if (e.button !== 0) return;
		if (getSketchMode()?.active) return;

		// Check if starting on empty space
		if (!isEmptySpace(e.clientX, e.clientY)) return;

		dragStartedOnEmpty = true;
		isDragging = false;
		dragStartX = e.clientX;
		dragStartY = e.clientY;
		dragCurrentX = e.clientX;
		dragCurrentY = e.clientY;
	}

	/**
	 * @param {MouseEvent} e
	 */
	function handleMouseMove(e) {
		if (!dragStartedOnEmpty) return;

		const dx = e.clientX - dragStartX;
		const dy = e.clientY - dragStartY;

		// Start box select after 5px of drag
		if (!isDragging && (Math.abs(dx) > 5 || Math.abs(dy) > 5)) {
			isDragging = true;

			// Disable OrbitControls during box drag
			const controls = getControlsObject();
			if (controls) controls.enabled = false;

			// Determine mode: left-to-right = window, right-to-left = crossing
			const mode = dx >= 0 ? 'window' : 'crossing';
			setBoxSelectState({
				active: true,
				startX: dragStartX,
				startY: dragStartY,
				endX: e.clientX,
				endY: e.clientY,
				mode
			});
		}

		if (isDragging) {
			dragCurrentX = e.clientX;
			dragCurrentY = e.clientY;

			const mode = (e.clientX - dragStartX) >= 0 ? 'window' : 'crossing';
			setBoxSelectState({
				active: true,
				startX: dragStartX,
				startY: dragStartY,
				endX: e.clientX,
				endY: e.clientY,
				mode
			});
		}
	}

	/**
	 * @param {MouseEvent} e
	 */
	function handleMouseUp(e) {
		if (!dragStartedOnEmpty) return;

		if (isDragging) {
			const mode = (e.clientX - dragStartX) >= 0 ? 'window' : 'crossing';
			const additive = e.shiftKey;
			performBoxSelect(dragStartX, dragStartY, e.clientX, e.clientY, mode, additive);

			// Re-enable OrbitControls
			const controls = getControlsObject();
			if (controls && !getSketchMode()?.active) {
				controls.enabled = true;
			}

			setBoxSelectState({ active: false, startX: 0, startY: 0, endX: 0, endY: 0, mode: 'window' });
		}

		isDragging = false;
		dragStartedOnEmpty = false;
	}

	// Reactive box select state for the overlay
	let boxState = $derived(getBoxSelectState());

	onMount(() => {
		const canvas = renderer?.domElement;
		if (!canvas) return;

		canvas.addEventListener('mousedown', handleMouseDown);
		window.addEventListener('mousemove', handleMouseMove);
		window.addEventListener('mouseup', handleMouseUp);

		return () => {
			canvas.removeEventListener('mousedown', handleMouseDown);
			window.removeEventListener('mousemove', handleMouseMove);
			window.removeEventListener('mouseup', handleMouseUp);
		};
	});
</script>

{#if boxState.active}
	{@const left = Math.min(boxState.startX, boxState.endX)}
	{@const top = Math.min(boxState.startY, boxState.endY)}
	{@const width = Math.abs(boxState.endX - boxState.startX)}
	{@const height = Math.abs(boxState.endY - boxState.startY)}
	<div
		class="box-select-overlay"
		class:crossing={boxState.mode === 'crossing'}
		style="left: {left}px; top: {top}px; width: {width}px; height: {height}px;"
		data-testid="box-select-overlay"
	></div>
{/if}

<style>
	.box-select-overlay {
		position: fixed;
		border: 2px solid #44aaff;
		background: rgba(68, 170, 255, 0.1);
		pointer-events: none;
		z-index: 1000;
	}
	.box-select-overlay.crossing {
		border-style: dashed;
		background: rgba(68, 255, 170, 0.1);
		border-color: #44ffaa;
	}
</style>
