/**
 * Sketch-mode zoom reversibility — zooming in then out should restore camera state.
 * This test is expected to FAIL against the current implementation, proving the bug
 * where sketch-mode zoom lerps the orbit target and doesn't reverse cleanly.
 */
import { test, expect } from './helpers/waffle-test.js';
import { zoom, drawCircle } from './helpers/canvas.js';
import { clickSketch, clickCircle } from './helpers/toolbar.js';
import { waitForEntityCount, collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

function vec3Length(v) {
	return Math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2);
}

function vec3Sub(a, b) {
	return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}

test.describe('sketch-mode zoom reversibility', () => {
	test('zoom in + zoom out preserves camera state in sketch mode', async ({ waffle }) => {
		const page = waffle.page;
		const tracker = collectCrashErrors(page);

		// Enter sketch mode on front plane and draw a circle
		await clickSketch(page);
		await clickCircle(page);
		await drawCircle(page, 0, 0, 80, 0);
		await waitForEntityCount(page, 2, 5000); // circle + center point

		// Let camera alignment settle after entering sketch mode
		await page.waitForTimeout(500);

		// Capture camera state BEFORE zoom.
		// In sketch/ortho mode, zoom changes frustumTop (camera.top), not camera.zoom.
		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());

		// Zoom in at canvas center
		await zoom(page, -300);
		await page.waitForTimeout(500);

		// Verify zoom actually changed something
		const mid = await page.evaluate(() => window.__waffle.getCameraState());

		// Check that at least one zoom indicator changed
		const frustumChanged = before.frustumTop !== null && mid.frustumTop !== null &&
			Math.abs(mid.frustumTop - before.frustumTop) > 0.00001;
		const targetMoved = vec3Length(vec3Sub(mid.target, before.target)) > 0.0001;
		const posMoved = vec3Length(vec3Sub(mid.position, before.position)) > 0.0001;
		expect(frustumChanged || targetMoved || posMoved).toBe(true);

		// Zoom back out by the same amount
		await zoom(page, 300);
		await page.waitForTimeout(500);

		// Capture camera state AFTER
		const after = await page.evaluate(() => window.__waffle.getCameraState());

		// Frustum values must be finite after zoom round-trip
		if (before.frustumTop !== null) {
			expect(Number.isFinite(after.frustumTop)).toBe(true);
		}
		// Frustum should restore within 20% (ortho zoom level)
		if (before.frustumTop !== null && after.frustumTop !== null &&
			Number.isFinite(before.frustumTop) && Number.isFinite(after.frustumTop) &&
			before.frustumTop > 0) {
			const frustumDrift = Math.abs(after.frustumTop - before.frustumTop) / before.frustumTop;
			expect(frustumDrift).toBeLessThan(0.2);
		}

		// Orbit target should return near where it started (key assertion for the lerp bug).
		// In ortho sketch mode, target gets lerped toward plane intersection on zoom-in
		// but the reverse lerp on zoom-out doesn't fully undo it.
		const targetDelta = vec3Sub(after.target, before.target);
		const targetDrift = vec3Length(targetDelta);
		const originalTargetDist = Math.max(vec3Length(before.target), 1);
		expect(targetDrift / originalTargetDist).toBeLessThan(0.2);

		// Camera position should also return close to original
		const posDelta = vec3Sub(after.position, before.position);
		const posDrift = vec3Length(posDelta);
		const originalPosDist = Math.max(vec3Length(before.position), 1);
		expect(posDrift / originalPosDist).toBeLessThan(0.2);

		// Camera position components must be finite (no NaN/Infinity)
		for (const v of after.position) {
			expect(Number.isFinite(v)).toBe(true);
		}
		for (const v of after.target) {
			expect(Number.isFinite(v)).toBe(true);
		}

		expectNoAnyCrash(tracker);
	});
});
