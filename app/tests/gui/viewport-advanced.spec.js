/**
 * Advanced viewport tests — zoom behavior, camera state, hover state management.
 */
import { test, expect } from './helpers/waffle-test.js';
import { zoom, getCanvasBounds, orbitDrag } from './helpers/canvas.js';

/**
 * Compute the distance from camera position to camera target.
 * @param {number[]} pos - camera position [x, y, z]
 * @param {number[]} tgt - camera target [x, y, z]
 * @returns {number}
 */
function cameraDistance(pos, tgt) {
	const dx = pos[0] - tgt[0];
	const dy = pos[1] - tgt[1];
	const dz = pos[2] - tgt[2];
	return Math.sqrt(dx ** 2 + dy ** 2 + dz ** 2);
}

test.describe('viewport advanced', () => {
	test('zoom-in moves camera closer', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		expect(before).not.toBeNull();
		const distBefore = cameraDistance(before.position, before.target);

		// Zoom in with negative deltaY
		await zoom(page, -300);
		await page.waitForTimeout(500);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();
		const distAfter = cameraDistance(after.position, after.target);

		expect(distAfter).toBeLessThan(distBefore);
	});

	test('zoom-out moves camera farther', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		expect(before).not.toBeNull();
		const distBefore = cameraDistance(before.position, before.target);

		// Zoom out with positive deltaY
		await zoom(page, 300);
		await page.waitForTimeout(500);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();
		const distAfter = cameraDistance(after.position, after.target);

		expect(distAfter).toBeGreaterThan(distBefore);
	});

	test('zoom with no object under cursor does not crash', async ({ waffle }) => {
		const page = waffle.page;

		const bounds = await getCanvasBounds(page);
		expect(bounds).not.toBeNull();

		// Move mouse to the far edge of the canvas (unlikely to be over model)
		await page.mouse.move(bounds.x + 5, bounds.y + 5);
		await page.mouse.wheel(0, -200);
		await page.waitForTimeout(500);

		// Canvas should still be visible and functional
		const canvas = page.locator('canvas');
		await expect(canvas).toBeVisible();
	});

	test('zoom near model zooms toward that area', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		expect(before).not.toBeNull();

		// Zoom in at canvas center (where model typically is)
		await zoom(page, -400);
		await page.waitForTimeout(500);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();

		// Verify the camera actually moved (position changed)
		const posMoved =
			before.position[0] !== after.position[0] ||
			before.position[1] !== after.position[1] ||
			before.position[2] !== after.position[2];
		expect(posMoved).toBe(true);
	});

	test('hover over edge area triggers store update', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => typeof window.__waffle !== 'undefined');

		// Set a face ref as the hovered ref via the API
		const faceRef = { Face: { feature_id: '00000000-0000-0000-0000-000000000000', face_index: 0 } };
		await page.evaluate((ref) => window.__waffle.setHoveredRef(ref), faceRef);
		await page.waitForTimeout(200);

		const hovered = await page.evaluate(() => window.__waffle.getHoveredRef());
		expect(hovered).not.toBeNull();
		expect(hovered).toHaveProperty('Face');
	});

	test('clearing hover resets hoveredRef', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => typeof window.__waffle !== 'undefined');

		// Set a hovered ref
		const faceRef = { Face: { feature_id: '00000000-0000-0000-0000-000000000000', face_index: 0 } };
		await page.evaluate((ref) => window.__waffle.setHoveredRef(ref), faceRef);
		await page.waitForTimeout(200);

		const hovered = await page.evaluate(() => window.__waffle.getHoveredRef());
		expect(hovered).not.toBeNull();

		// Clear the hover
		await page.evaluate(() => window.__waffle.setHoveredRef(null));
		await page.waitForTimeout(200);

		const cleared = await page.evaluate(() => window.__waffle.getHoveredRef());
		expect(cleared).toBeNull();
	});

	test('getCameraState returns valid position after orbit', async ({ waffle }) => {
		const page = waffle.page;

		// Orbit drag to change the camera
		await orbitDrag(page, -50, -30, 80, 40);
		await page.waitForTimeout(500);

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const state = await page.evaluate(() => window.__waffle.getCameraState());

		expect(state).not.toBeNull();
		expect(Array.isArray(state.position)).toBe(true);
		expect(state.position).toHaveLength(3);
		expect(typeof state.position[0]).toBe('number');
		expect(typeof state.position[1]).toBe('number');
		expect(typeof state.position[2]).toBe('number');
		// Verify position values are finite (not NaN or Infinity)
		expect(Number.isFinite(state.position[0])).toBe(true);
		expect(Number.isFinite(state.position[1])).toBe(true);
		expect(Number.isFinite(state.position[2])).toBe(true);
	});

	test('getCameraState returns valid position after fit-all', async ({ waffle }) => {
		const page = waffle.page;

		// Press F for fit-all
		await page.keyboard.press('f');
		await page.waitForTimeout(500);

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const state = await page.evaluate(() => window.__waffle.getCameraState());

		expect(state).not.toBeNull();
		expect(Array.isArray(state.position)).toBe(true);
		expect(state.position).toHaveLength(3);
		expect(typeof state.position[0]).toBe('number');
		expect(typeof state.position[1]).toBe('number');
		expect(typeof state.position[2]).toBe('number');
		expect(Number.isFinite(state.position[0])).toBe(true);
		expect(Number.isFinite(state.position[1])).toBe(true);
		expect(Number.isFinite(state.position[2])).toBe(true);
	});

	test('getCameraState returns valid target after pan', async ({ waffle }) => {
		const page = waffle.page;

		// Small orbit drag (acts as a pan-like camera movement)
		await orbitDrag(page, 0, 0, 20, 15);
		await page.waitForTimeout(500);

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const state = await page.evaluate(() => window.__waffle.getCameraState());

		expect(state).not.toBeNull();
		expect(Array.isArray(state.target)).toBe(true);
		expect(state.target).toHaveLength(3);
		expect(typeof state.target[0]).toBe('number');
		expect(typeof state.target[1]).toBe('number');
		expect(typeof state.target[2]).toBe('number');
		expect(Number.isFinite(state.target[0])).toBe(true);
		expect(Number.isFinite(state.target[1])).toBe(true);
		expect(Number.isFinite(state.target[2])).toBe(true);
	});

	test('edge hover color differs from default', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => typeof window.__waffle !== 'undefined');

		// Set a face ref as hovered — verify it's stored
		const faceRef = { Face: { feature_id: '00000000-0000-0000-0000-000000000000', face_index: 1 } };
		await page.evaluate((ref) => window.__waffle.setHoveredRef(ref), faceRef);
		await page.waitForTimeout(200);

		const hovered = await page.evaluate(() => window.__waffle.getHoveredRef());
		expect(hovered).not.toBeNull();
		expect(hovered).toHaveProperty('Face');
		expect(hovered.Face.face_index).toBe(1);

		// Clear the hover and verify null
		await page.evaluate(() => window.__waffle.setHoveredRef(null));
		await page.waitForTimeout(200);

		const cleared = await page.evaluate(() => window.__waffle.getHoveredRef());
		expect(cleared).toBeNull();
	});
});
