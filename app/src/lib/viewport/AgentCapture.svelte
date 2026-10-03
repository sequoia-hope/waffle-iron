<script>
	/**
	 * Agent `viewport_capture` (specs/waffle_mcp_server.md §2.5 Q4, extended by
	 * specs/agent_mechanical_design.md §9, increment V1): answers the synchronous
	 * 'waffle-agent-capture' window event with a PNG plus the legend and the
	 * labels drawn into it.
	 *
	 * This component only gathers what the pass needs from Threlte and the store
	 * and hands it to `capture.js`; the render pass, the overrides and the label
	 * placement are there, so a viewer page or a test can exercise them without a
	 * Svelte tree. The user's camera, selection and body visibility are not
	 * touched — the pass builds its own camera and restores every override it
	 * applies (§9.2).
	 */
	import { useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import { getCameraRefs, getMeshes, getBodies, isBodyVisible } from '$lib/engine/store.svelte.js';
	import { CaptureError, renderCapture } from './capture.js';

	const { renderer, scene, camera } = useThrelte();

	/** The render list as the capture needs it: names beside face ranges. */
	function captureBodies() {
		const names = new Map();
		for (const b of getBodies()) names.set(b.bodyId, b.name ?? null);
		return getMeshes()
			.filter((m) => isBodyVisible(m.bodyId))
			.map((m) => ({
				bodyId: m.bodyId ?? null,
				featureId: m.featureId ?? null,
				name: (m.bodyId ? names.get(m.bodyId) : null) ?? m.name ?? null,
				faceRanges: m.faceRanges ?? []
			}));
	}

	/** @param {CustomEvent} e */
	function onCapture(e) {
		const cam = camera.current;
		const canvas = renderer?.domElement;
		if (!cam || !canvas) return;
		if (canvas.width === 0 || canvas.height === 0) {
			e.detail.unavailable = 'The viewport has no visible area in this tab.';
			return;
		}
		const { controls } = getCameraRefs();
		const liveTarget = controls?.target ? controls.target.clone() : new THREE.Vector3();
		const background =
			getComputedStyle(document.documentElement).getPropertyValue('--viewport-bg').trim() || '#000000';
		try {
			e.detail.result = renderCapture({
				renderer,
				scene,
				liveCamera: cam,
				liveTarget,
				bodies: captureBodies(),
				background,
				args: e.detail.args ?? {}
			});
		} catch (err) {
			if (err instanceof CaptureError) {
				e.detail.refused = { code: err.code, detail: err.detail, details: err.details };
				return;
			}
			throw err;
		}
	}

	onMount(() => {
		window.addEventListener('waffle-agent-capture', /** @type {EventListener} */ (onCapture));
		return () => window.removeEventListener('waffle-agent-capture', /** @type {EventListener} */ (onCapture));
	});
</script>
