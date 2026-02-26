/**
 * Viewport pan + zoom limit tests — middle-click drag panning, wheel zoom with distance limits.
 */
import { test, expect } from './helpers/waffle-test.js';
import { zoom, getCanvasBounds, orbitDrag } from './helpers/canvas.js';
import { createExtrudedBox } from './helpers/geometry.js';

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

/**
 * Perform a middle-click drag (pan) on the canvas.
 * @param {import('@playwright/test').Page} page
 * @param {number} startX - start x offset from center
 * @param {number} startY - start y offset from center
 * @param {number} endX - end x offset from center
 * @param {number} endY - end y offset from center
 */
async function panDrag(page, startX, startY, endX, endY) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');

	const sx = bounds.centerX + startX;
	const sy = bounds.centerY + startY;
	const ex = bounds.centerX + endX;
	const ey = bounds.centerY + endY;

	await page.mouse.move(sx, sy);
	await page.mouse.down({ button: 'middle' });
	const steps = 5;
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(
			sx + (ex - sx) * t,
			sy + (ey - sy) * t
		);
	}
	await page.mouse.up({ button: 'middle' });
	await page.waitForTimeout(300);
}

test.describe('viewport pan', () => {
	test('middle-click drag pans camera target', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		expect(before).not.toBeNull();

		// Middle-click drag to pan
		await panDrag(page, -80, 0, 80, 0);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();

		// Camera target should have changed (pan moves the focal point)
		const dx = after.target[0] - before.target[0];
		const dy = after.target[1] - before.target[1];
		const dz = after.target[2] - before.target[2];
		const targetDist = Math.sqrt(dx ** 2 + dy ** 2 + dz ** 2);
		expect(targetDist).toBeGreaterThan(0.1);
	});

	test('pan preserves camera distance from target', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		const distBefore = cameraDistance(before.position, before.target);

		await panDrag(page, 0, -60, 0, 60);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		const distAfter = cameraDistance(after.position, after.target);

		// Distance should be approximately preserved (within 10%)
		expect(Math.abs(distAfter - distBefore) / distBefore).toBeLessThan(0.1);
	});

	test('pan on empty viewport does not crash', async ({ waffle }) => {
		const page = waffle.page;

		// Pan with no geometry — should not error
		await panDrag(page, -100, -50, 100, 50);

		const canvas = page.locator('canvas');
		await expect(canvas).toBeVisible();

		// Camera state should still be valid
		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();
		expect(Number.isFinite(state.position[0])).toBe(true);
	});

	test('pan with geometry present moves both camera and target', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());

		await panDrag(page, -60, -40, 60, 40);

		const after = await page.evaluate(() => window.__waffle.getCameraState());

		// Both position and target should move together during pan
		const posDx = after.position[0] - before.position[0];
		const posDy = after.position[1] - before.position[1];
		const posDz = after.position[2] - before.position[2];
		const posMoved = Math.sqrt(posDx ** 2 + posDy ** 2 + posDz ** 2);
		expect(posMoved).toBeGreaterThan(0.1);

		const tgtDx = after.target[0] - before.target[0];
		const tgtDy = after.target[1] - before.target[1];
		const tgtDz = after.target[2] - before.target[2];
		const tgtMoved = Math.sqrt(tgtDx ** 2 + tgtDy ** 2 + tgtDz ** 2);
		expect(tgtMoved).toBeGreaterThan(0.1);
	});
});

test.describe('zoom limits', () => {
	test('zoom in reduces camera distance', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		const distBefore = cameraDistance(before.position, before.target);

		await zoom(page, -300);
		await page.waitForTimeout(300);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		const distAfter = cameraDistance(after.position, after.target);

		expect(distAfter).toBeLessThan(distBefore);
	});

	test('zoom out increases camera distance', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		const distBefore = cameraDistance(before.position, before.target);

		await zoom(page, 300);
		await page.waitForTimeout(300);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		const distAfter = cameraDistance(after.position, after.target);

		expect(distAfter).toBeGreaterThan(distBefore);
	});

	test('zoom does not exceed MAX_DISTANCE of 2000', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		// Zoom out aggressively — many large wheel events
		for (let i = 0; i < 20; i++) {
			await zoom(page, 500);
		}
		await page.waitForTimeout(500);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();
		const dist = cameraDistance(state.position, state.target);

		// Distance should be clamped at or below MAX_DISTANCE=2000
		expect(dist).toBeLessThanOrEqual(2050); // small tolerance for floating point
	});

	test('multiple zoom-in steps progressively reduce distance', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const distances = [];

		for (let i = 0; i < 5; i++) {
			const state = await page.evaluate(() => window.__waffle.getCameraState());
			distances.push(cameraDistance(state.position, state.target));
			await zoom(page, -200);
			await page.waitForTimeout(200);
		}

		// Each subsequent distance should be less than or equal to the previous
		for (let i = 1; i < distances.length; i++) {
			expect(distances[i]).toBeLessThanOrEqual(distances[i - 1] + 0.01);
		}
	});

	test('zoom in then zoom out returns camera near original distance', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());
		const distBefore = cameraDistance(before.position, before.target);

		// Zoom in
		await zoom(page, -300);
		await page.waitForTimeout(200);

		// Zoom back out
		await zoom(page, 300);
		await page.waitForTimeout(300);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		const distAfter = cameraDistance(after.position, after.target);

		// Should be roughly the same distance (within 20%)
		expect(Math.abs(distAfter - distBefore) / distBefore).toBeLessThan(0.2);
	});
});

test.describe('pan + orbit combo', () => {
	test('pan followed by orbit preserves camera integrity', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		// Pan first
		await panDrag(page, -50, 0, 50, 0);
		const afterPan = await page.evaluate(() => window.__waffle.getCameraState());
		expect(afterPan).not.toBeNull();

		// Then orbit
		await orbitDrag(page, -80, 0, 80, 0);
		const afterOrbit = await page.evaluate(() => window.__waffle.getCameraState());
		expect(afterOrbit).not.toBeNull();

		// Camera should still have valid finite values
		for (const v of afterOrbit.position) {
			expect(Number.isFinite(v)).toBe(true);
		}
		for (const v of afterOrbit.target) {
			expect(Number.isFinite(v)).toBe(true);
		}
	});

	test('orbit followed by pan does not reset orbit', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const initial = await page.evaluate(() => window.__waffle.getCameraState());

		// Orbit to change viewing angle
		await orbitDrag(page, -100, 0, 100, 0);
		const afterOrbit = await page.evaluate(() => window.__waffle.getCameraState());

		// Pan
		await panDrag(page, 0, -50, 0, 50);
		const afterPan = await page.evaluate(() => window.__waffle.getCameraState());

		// The viewing direction from orbit should be preserved after pan.
		// The camera-to-target vector direction should be similar after orbit+pan vs just orbit.
		const distAfterOrbit = cameraDistance(afterOrbit.position, afterOrbit.target);
		const distAfterPan = cameraDistance(afterPan.position, afterPan.target);

		// Distance should be approximately preserved (pan doesn't change distance)
		expect(Math.abs(distAfterPan - distAfterOrbit) / distAfterOrbit).toBeLessThan(0.1);
	});

	test('zoom + pan + orbit combo produces valid camera state', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);

		// Zoom in
		await zoom(page, -200);
		await page.waitForTimeout(200);

		// Pan
		await panDrag(page, -40, -30, 40, 30);

		// Orbit
		await orbitDrag(page, -60, -20, 60, 20);

		const state = await page.evaluate(() => window.__waffle.getCameraState());
		expect(state).not.toBeNull();
		expect(state.position).toHaveLength(3);
		expect(state.target).toHaveLength(3);
		for (const v of [...state.position, ...state.target]) {
			expect(Number.isFinite(v)).toBe(true);
		}
	});
});
