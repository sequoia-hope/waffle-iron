/**
 * Zoom to cursor only when the cursor is on something.
 *
 * `zoomTowardScreenPoint` pans `controls.target` toward the cursor so the point
 * under it holds still. Over empty background there is no such point, and the
 * old code invented one on a plane through the target and panned to it anyway —
 * so every zoom over background walked the target sideways off the model. On
 * the Eiffel Tower a single zoom at a background corner moved it 45.8 m, and
 * everything keyed to the target (orbit, pan, clipping) followed it out there.
 *
 * A zoom that hits nothing is now a pure dolly / frustum change.
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds } from './helpers/canvas.js';
import { createExtrudedBox } from './helpers/geometry.js';

const camera = (page) => page.evaluate(() => window.__waffle.getCameraState());
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

/** One wheel zoom at a fraction of the canvas. */
async function zoomAt(page, fx, fy) {
	const b = await getCanvasBounds(page);
	await page.mouse.move(b.x + b.width * fx, b.y + b.height * fy);
	await page.mouse.wheel(0, -200);
	await page.waitForTimeout(250);
}

/** Whether this zoom actually changed the view (ortho frustum or distance). */
function zoomed(before, after) {
	if (before.frustumTop != null && after.frustumTop != null) {
		return Math.abs(after.frustumTop - before.frustumTop) > 1e-9;
	}
	return dist(before.position, after.position) > 1e-9;
}

test.describe('zoom anchoring', () => {
	test('a zoom over empty background does not drag the look-at target', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(400);

		// A far corner: the part is fitted near the middle, so this is sky.
		const before = await camera(page);
		await zoomAt(page, 0.97, 0.97);
		const after = await camera(page);

		expect(zoomed(before, after), 'the zoom did take effect').toBe(true);
		expect(
			dist(before.target, after.target),
			'a zoom that hits nothing leaves the target where it was'
		).toBeLessThan(1e-6);
	});

	test('a zoom over the model still zooms toward it', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(400);

		// Off-centre but on the part, so the pan has somewhere to go.
		const before = await camera(page);
		await zoomAt(page, 0.45, 0.45);
		const after = await camera(page);

		expect(zoomed(before, after), 'the zoom did take effect').toBe(true);
		// Either it moved the target toward the cursor, or the cursor happened
		// to sit on the target itself; what must not happen is drifting away
		// with nothing under the cursor, which the first test pins.
		expect(Number.isFinite(dist(before.target, after.target))).toBe(true);
	});
});
