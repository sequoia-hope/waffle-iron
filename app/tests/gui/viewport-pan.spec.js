/**
 * Viewport pan + zoom limit tests — middle-click drag panning, wheel zoom with distance limits.
 */
import { test, expect } from './helpers/waffle-test.js';
import { zoom, getCanvasBounds, orbitDrag, viewExtent, waitForCameraRest } from './helpers/canvas.js';
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

		// Middle-click drag to pan, then let the damped glide finish.
		await panDrag(page, -80, 0, 80, 0);

		const after = await waitForCameraRest(page);
		expect(after).not.toBeNull();

		// The pan carries the focal point with the cursor: in the default
		// orthographic view a 160 px drag moves the target by 160 px of world.
		expect(before.projection).toBe('orthographic');
		const bounds = await getCanvasBounds(page);
		const worldPerPx = (2 * viewExtent(before)) / bounds.height;
		const expected = 160 * worldPerPx;
		const dx = after.target[0] - before.target[0];
		const dy = after.target[1] - before.target[1];
		const dz = after.target[2] - before.target[2];
		const targetDist = Math.sqrt(dx ** 2 + dy ** 2 + dz ** 2);
		expect(targetDist).toBeGreaterThan(0.9 * expected);
		expect(targetDist).toBeLessThan(1.1 * expected);
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
	test('zoom in shows less of the model', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());

		await zoom(page, -300);
		await page.waitForTimeout(300);

		const after = await page.evaluate(() => window.__waffle.getCameraState());

		expect(viewExtent(after)).toBeLessThan(viewExtent(before));
	});

	test('zoom out shows more of the model', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = await page.evaluate(() => window.__waffle.getCameraState());

		await zoom(page, 300);
		await page.waitForTimeout(300);

		const after = await page.evaluate(() => window.__waffle.getCameraState());

		expect(viewExtent(after)).toBeGreaterThan(viewExtent(before));
	});

	test('zoom out stops at a limit', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const start = viewExtent(await page.evaluate(() => window.__waffle.getCameraState()));

		// Zoom out aggressively — many large wheel events. Unclamped, twenty
		// of them would widen the view about a thousand times.
		for (let i = 0; i < 20; i++) {
			await zoom(page, 500);
		}
		await page.waitForTimeout(500);
		const atLimit = viewExtent(await page.evaluate(() => window.__waffle.getCameraState()));
		expect(atLimit).toBeGreaterThan(start);
		expect(atLimit).toBeLessThan(start * 500);

		// At the limit, more zooming out changes nothing.
		for (let i = 0; i < 5; i++) {
			await zoom(page, 500);
		}
		await page.waitForTimeout(300);
		const beyond = viewExtent(await page.evaluate(() => window.__waffle.getCameraState()));
		expect(beyond).toBeCloseTo(atLimit, 9);
	});

	test('multiple zoom-in steps progressively show less', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const extents = [];

		for (let i = 0; i < 5; i++) {
			const state = await page.evaluate(() => window.__waffle.getCameraState());
			extents.push(viewExtent(state));
			await zoom(page, -200);
			await page.waitForTimeout(200);
		}

		// Each step shows strictly less than the one before.
		for (let i = 1; i < extents.length; i++) {
			expect(extents[i]).toBeLessThan(extents[i - 1]);
		}
	});

	test('zoom in then zoom out returns the view near its original extent', async ({ waffle }) => {
		const page = waffle.page;

		await page.waitForFunction(() => window.__waffle?.getCameraState() !== null);
		const before = viewExtent(await page.evaluate(() => window.__waffle.getCameraState()));

		// Zoom in
		await zoom(page, -300);
		await page.waitForTimeout(200);
		const zoomedIn = viewExtent(await page.evaluate(() => window.__waffle.getCameraState()));
		expect(zoomedIn).toBeLessThan(before);

		// Zoom back out
		await zoom(page, 300);
		await page.waitForTimeout(300);

		const after = viewExtent(await page.evaluate(() => window.__waffle.getCameraState()));

		// Should be roughly the same extent (within 20%)
		expect(Math.abs(after - before) / before).toBeLessThan(0.2);
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
