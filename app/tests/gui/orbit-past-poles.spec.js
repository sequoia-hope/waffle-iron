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

		// Position should change significantly (not stuck). Two 200 px drags
		// turn the camera well over a quarter turn, so it ends more than half
		// an orbit radius from where it was.
		const dist = Math.sqrt(
			(after[0] - before[0]) ** 2 +
			(after[1] - before[1]) ** 2 +
			(after[2] - before[2]) ** 2
		);
		const radius = Math.hypot(before[0], before[1], before[2]);
		expect(radius).toBeGreaterThan(0);
		expect(dist).toBeGreaterThan(0.5 * radius);
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

		// The RIGHT button orbits.
		await page.mouse.move(cx, cy + 100);
		await page.mouse.down({ button: 'right' });
		for (let i = 1; i <= totalSteps; i++) {
			await page.mouse.move(cx, cy + 100 - i * stepSize);
			await page.waitForTimeout(20);
			const state = await page.evaluate(() => window.__waffle.getCameraState());
			positions.push(state.position);
		}
		await page.mouse.up({ button: 'right' });

		// The drag really orbited: the camera ended somewhere else.
		const radius = Math.hypot(...initial.position.map((v, i) => v - initial.target[i]));
		const [fx, fy, fz] = positions[positions.length - 1];
		const travelled = positions.some(
			([x, y, z]) => Math.hypot(x - initial.position[0], y - initial.position[1], z - initial.position[2]) > 0.5 * radius
		);
		expect(travelled, 'the drag orbited the camera').toBe(true);
		expect(Number.isFinite(fx + fy + fz)).toBe(true);

		// Check that no consecutive pair has a position jump. Each 8 px step
		// turns the camera a few degrees, so it moves a small fraction of the
		// orbit radius. A discontinuity (a 180° flip) would be 2 radii.
		let maxJump = 0;
		for (let i = 1; i < positions.length; i++) {
			const [x1, y1, z1] = positions[i - 1];
			const [x2, y2, z2] = positions[i];
			const dist = Math.sqrt((x2 - x1) ** 2 + (y2 - y1) ** 2 + (z2 - z1) ** 2);
			if (dist > maxJump) maxJump = dist;
		}

		expect(maxJump, 'camera position must be continuous (no jumps)').toBeLessThan(0.3 * radius);
	});
});
