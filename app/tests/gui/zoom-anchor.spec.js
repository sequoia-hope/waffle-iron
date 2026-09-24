/**
 * Zoom goes toward the CURSOR — always.
 *
 * `zoomTowardScreenPoint` moves the camera (and the look-at target with it) so
 * the world point under the pointer stays under the pointer. That is the whole
 * gesture, and it applies over empty background too: there the anchor is where
 * the cursor ray crosses the FOCAL plane — through the target, normal along the
 * view — which is what the pointer is aiming at, at the depth you are looking.
 *
 * A previous attempt made a miss a pure dolly, to stop the target wandering off
 * a tall sparse model. That cured the wrong thing: the target moving is the
 * gesture working. What must not follow it is the ORBIT, and that is
 * `controls.orbitPivot`'s job — re-anchored on the model under the cursor at
 * every rotate.
 *
 * So the invariants here are:
 *  1. over the model, SCREEN ANCHORING — what was under the pointer is still
 *     under the pointer;
 *  2. over background, the view still moves SIDEWAYS toward the pointer, not
 *     straight down the view axis.
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds } from './helpers/canvas.js';
import { createExtrudedBox } from './helpers/geometry.js';
import { worldToScreen } from './helpers/worldToScreen.js';

const camera = (page) => page.evaluate(() => window.__waffle.getCameraState());
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const norm = (a) => {
	const l = Math.hypot(a[0], a[1], a[2]) || 1;
	return [a[0] / l, a[1] / l, a[2] / l];
};

/** Whether this zoom actually changed the view (ortho frustum or distance). */
function zoomed(before, after) {
	if (before.frustumTop != null && after.frustumTop != null) {
		return Math.abs(after.frustumTop - before.frustumTop) > 1e-9;
	}
	return dist(before.position, after.position) > 1e-9;
}

test.describe('zoom anchoring', () => {
	test('a zoom over the model keeps that point under the cursor', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(400);

		// The centre of the box's top face: a point that is genuinely ON the
		// part, and whose pixel the cursor ray therefore hits.
		const box = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(box, 'the box is in the scene').not.toBeNull();
		const anchor = [box.center[0], box.center[1], box.max[2]];

		const s0 = await worldToScreen(page, anchor);
		await page.mouse.move(s0.x, s0.y);
		const before = await camera(page);
		await page.mouse.wheel(0, -200);
		await page.waitForTimeout(250);
		const after = await camera(page);
		expect(zoomed(before, after), 'the zoom did take effect').toBe(true);

		const s1 = await worldToScreen(page, anchor);
		expect(
			Math.hypot(s1.x - s0.x, s1.y - s0.y),
			'the anchored point stayed where the cursor was'
		).toBeLessThan(8);
	});

	test('a zoom over empty background still moves toward the cursor', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(400);

		// A far corner: the part is fitted near the middle, so this is sky. The
		// zoom must go THAT WAY — a pure dolly down the view axis would leave
		// the corner exactly where it was, which is the bug this pins.
		const b = await getCanvasBounds(page);
		await page.mouse.move(b.x + b.width * 0.97, b.y + b.height * 0.97);
		const before = await camera(page);
		await page.mouse.wheel(0, -200);
		await page.waitForTimeout(250);
		const after = await camera(page);

		expect(zoomed(before, after), 'the zoom did take effect').toBe(true);
		// The target moved, and it moved SIDEWAYS: the displacement has a real
		// component perpendicular to the view direction.
		const moved = sub(after.target, before.target);
		expect(Math.hypot(...moved), 'the view moved').toBeGreaterThan(1e-6);
		const view = norm(sub(before.target, before.position));
		const lateral = sub(moved, view.map((c) => c * dot(moved, view)));
		expect(
			Math.hypot(...lateral) / Math.hypot(...moved),
			'the move is toward the cursor, not straight in'
		).toBeGreaterThan(0.2);
	});
});
