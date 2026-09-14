<script>
	/**
	 * Agent `viewport_capture` (specs/waffle_mcp_server.md §2.5, Q4): answers the
	 * synchronous 'waffle-agent-capture' window event with a PNG of the current
	 * view. It renders one frame and reads the canvas in the same task, so the
	 * drawing buffer is still intact without `preserveDrawingBuffer`. The WebGL
	 * canvas is transparent (the viewport's CSS paints the ground), so the frame
	 * is composited over --viewport-bg. The camera is not moved.
	 */
	import { useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';

	const { renderer, scene, camera } = useThrelte();

	/** @param {CustomEvent} e */
	function onCapture(e) {
		const cam = camera.current;
		const canvas = renderer?.domElement;
		if (!cam || !canvas) return;
		if (canvas.width === 0 || canvas.height === 0) {
			e.detail.unavailable = 'The viewport has no visible area in this tab.';
			return;
		}
		const scale = Math.min(1, e.detail.maxEdge / Math.max(canvas.width, canvas.height));
		const width = Math.max(1, Math.round(canvas.width * scale));
		const height = Math.max(1, Math.round(canvas.height * scale));
		const out = document.createElement('canvas');
		out.width = width;
		out.height = height;
		const ctx = /** @type {CanvasRenderingContext2D} */ (out.getContext('2d'));
		ctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--viewport-bg').trim() || '#000000';
		ctx.fillRect(0, 0, width, height);
		renderer.render(scene, cam);
		ctx.imageSmoothingQuality = 'high';
		ctx.drawImage(canvas, 0, 0, width, height);
		const url = out.toDataURL('image/png');
		e.detail.image = { png: url.slice(url.indexOf(',') + 1), width, height };
	}

	onMount(() => {
		window.addEventListener('waffle-agent-capture', /** @type {EventListener} */ (onCapture));
		return () => window.removeEventListener('waffle-agent-capture', /** @type {EventListener} */ (onCapture));
	});
</script>
