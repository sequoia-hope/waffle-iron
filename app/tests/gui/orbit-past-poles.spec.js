/**
 * Orbit past poles — tests that camera can orbit continuously past
 * the north/south poles without getting stuck or jumping.
 *
 * Fix: OrbitControls.update() is replaced with quaternion-based rotation
 * that eliminates the spherical coordinate singularity at the poles.
 * camera.up is co-rotated with the position to keep lookAt() stable.
 */
import { test, expect } from './helpers/waffle-test.js';
import { orbitDrag, getCanvasBounds } from './helpers/canvas.js';

test.describe('orbit past poles', () => {
	test('camera orbits past the south pole', async ({ waffle }) => {
		const page = waffle.page;

		const initial = await page.evaluate(() => window.__waffle.getCameraState());
		expect(initial).not.toBeNull();
		expect(initial.position[1] - initial.target[1]).toBeGreaterThan(0);

		// 6 upward drags should orbit past both poles and return above the target
		for (let i = 0; i < 6; i++) {
			await orbitDrag(page, 0, 100, 0, -100);
		}

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after).not.toBeNull();
		expect(after.position[1] - after.target[1]).toBeGreaterThan(0);
	});

	test('camera movement is not stuck near the south pole', async ({ waffle }) => {
		const page = waffle.page;

		// Two drags approach the south pole
		await orbitDrag(page, 0, 100, 0, -100);
		await orbitDrag(page, 0, 100, 0, -100);

		const before = await page.evaluate(() => {
			const s = window.__waffle.getCameraState();
			return s ? s.position : null;
		});
		expect(before).not.toBeNull();

		// Two more drags should move the camera substantially
		await orbitDrag(page, 0, 100, 0, -100);
		await orbitDrag(page, 0, 100, 0, -100);

		const after = await page.evaluate(() => {
			const s = window.__waffle.getCameraState();
			return s ? s.position : null;
		});
		expect(after).not.toBeNull();

		// Position should change significantly (not stuck)
		const dist = Math.sqrt(
			(after[0] - before[0]) ** 2 +
			(after[1] - before[1]) ** 2 +
			(after[2] - before[2]) ** 2
		);
		expect(dist).toBeGreaterThan(10);
	});

	test('orbit is continuous with no position jumps through poles', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		const cx = bounds.centerX;
		const cy = bounds.centerY;

		// Do a slow continuous upward drag, recording position at each step.
		// This traces the camera through pole boundaries.
		const positions = [];
		const initial = await page.evaluate(() => window.__waffle.getCameraState());
		positions.push(initial.position);

		const totalSteps = 50;
		const stepSize = 8;

		await page.mouse.move(cx, cy + 100);
		await page.mouse.down();
		for (let i = 1; i <= totalSteps; i++) {
			await page.mouse.move(cx, cy + 100 - i * stepSize);
			await page.waitForTimeout(20);
			const state = await page.evaluate(() => window.__waffle.getCameraState());
			positions.push(state.position);
		}
		await page.mouse.up();

		// Check that no consecutive pair has a position jump.
		// On a sphere of radius ~52, each 8px step should move the camera
		// at most ~8 units. A discontinuity (180° flip) would be ~104 units.
		let maxJump = 0;
		for (let i = 1; i < positions.length; i++) {
			const [x1, y1, z1] = positions[i - 1];
			const [x2, y2, z2] = positions[i];
			const dist = Math.sqrt((x2 - x1) ** 2 + (y2 - y1) ** 2 + (z2 - z1) ** 2);
			if (dist > maxJump) maxJump = dist;
		}

		expect(maxJump, 'camera position must be continuous (no jumps)').toBeLessThan(15);
	});
});
