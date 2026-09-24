/**
 * The orbit pivot is re-anchored at the start of every rotate.
 *
 * `controls.target` is what the camera looks AT, and zoom-to-cursor moves it
 * wherever you point — including into empty space on a model that is mostly
 * air. Orbit used to turn about that same point, so a few zooms over
 * background left the model swinging about somewhere outside itself (reported
 * on the Eiffel Tower example, 2026-09-24). Orbit now turns about
 * `controls.orbitPivot`, re-anchored at each rotate to the geometry under the
 * cursor and falling back to the visible model's bounding-box centre — never
 * to whatever `target` has drifted to.
 */
import { test, expect } from './helpers/waffle-test.js';
import { orbitDrag } from './helpers/canvas.js';
import { createExtrudedBox } from './helpers/geometry.js';
import { getMeshBoundingBox } from './helpers/state.js';

const camera = (page) => page.evaluate(() => window.__waffle.getCameraState());

/** Put the look-at target somewhere the model is not, as a drifting zoom does. */
async function dragTargetAway(page, target) {
	const cam = await camera(page);
	await page.evaluate(
		([position, up, t]) => {
			window.dispatchEvent(
				new CustomEvent('waffle-restore-camera', { detail: { position, up, target: t } })
			);
		},
		[cam.position, cam.up, target]
	);
	await page.waitForTimeout(150);
}

test.describe('orbit pivot', () => {
	test('a rotate turns about the model, not about a target that has drifted off it', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);

		const away = [5, 5, 5]; // metres from a part measured in millimetres
		await dragTargetAway(page, away);
		expect((await camera(page)).target).toEqual(away);

		await orbitDrag(page, 0, 0, 110, 25);

		const after = await camera(page);
		expect(after.orbitPivot, 'the rotate anchored a pivot').not.toBeNull();
		expect(after.orbitPivot, 'the pivot is not the drifted target').not.toEqual(away);

		// It is ON the model: inside the bounding box of what is rendered,
		// with a tolerance for the box's own extent.
		const bbox = await getMeshBoundingBox(page);
		expect(bbox, 'the part has a bounding box').not.toBeNull();
		const slack = Math.max(bbox.max[0] - bbox.min[0], bbox.max[1] - bbox.min[1], bbox.max[2] - bbox.min[2]) * 0.05;
		for (const i of [0, 1, 2]) {
			expect(after.orbitPivot[i]).toBeGreaterThanOrEqual(bbox.min[i] - slack);
			expect(after.orbitPivot[i]).toBeLessThanOrEqual(bbox.max[i] + slack);
		}
	});

	test('orbiting holds the point it turns about', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await orbitDrag(page, 0, 0, 90, 30);

		const first = await camera(page);
		expect(first.orbitPivot).not.toBeNull();
		const distance = (c) =>
			Math.hypot(
				c.position[0] - c.orbitPivot[0],
				c.position[1] - c.orbitPivot[1],
				c.position[2] - c.orbitPivot[2]
			);
		const before = distance(first);

		// A second rotate from the same place keeps the same pivot, and orbiting
		// about a point leaves the camera the same distance from it.
		await orbitDrag(page, 0, 0, -70, 40);
		const second = await camera(page);
		expect(second.orbitPivot).toEqual(first.orbitPivot);
		expect(distance(second)).toBeCloseTo(before, 4);
	});
});
