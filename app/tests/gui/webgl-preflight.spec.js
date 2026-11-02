/**
 * Can this browser make a WebGL context at all?
 *
 * The viewport is three.js, so without WebGL `Canvas.svelte` throws, the
 * engine never reports ready, and EVERY spec dies in the fixture at 60 s.
 * On the nightly cross-browser run that turned one environment fault into
 * 193 identical failures and a 2h17m job (Firefox on a GPU-less runner,
 * 2026-10-29 → 11-02). The workflow runs this spec first, so the same fault
 * is one failure in seconds, carrying the browser's own reason.
 *
 * Deliberately not the app and not the `waffle` fixture: a blank page and a
 * canvas, so a red here can only be the browser's graphics stack.
 */
import { test, expect } from '@playwright/test';

test('the browser can create a WebGL context', async ({ page }) => {
	await page.setContent('<canvas id="c" width="64" height="64"></canvas>');
	const result = await page.evaluate(() => {
		const canvas = document.getElementById('c');
		let reason = '';
		canvas.addEventListener('webglcontextcreationerror', (e) => {
			reason = e.statusMessage;
		});
		const gl = canvas.getContext('webgl2') ?? canvas.getContext('webgl');
		if (!gl) return { ok: false, reason };
		const info = gl.getExtension('WEBGL_debug_renderer_info');
		return {
			ok: true,
			version: gl.getParameter(gl.VERSION),
			renderer: gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER)
		};
	});
	console.log('WebGL preflight:', JSON.stringify(result));
	expect(result.ok, `WebGL context refused: ${result.reason}`).toBe(true);
});
