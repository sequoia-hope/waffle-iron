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
import { orbitDrag, getCanvasBounds, viewExtent } from './helpers/canvas.js';
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
		const bounds = await getCanvasBounds(page);
		const distance = (c) =>
			Math.hypot(
				c.position[0] - c.orbitPivot[0],
				c.position[1] - c.orbitPivot[1],
				c.position[2] - c.orbitPivot[2]
			);

		// One rotate gesture from the canvas centre, read while the button is
		// still down: the press anchors the pivot, and the drag turns the
		// camera about it.
		const rotate = async (dx, dy) => {
			await page.mouse.move(bounds.centerX, bounds.centerY);
			await page.mouse.down({ button: 'right' });
			const pressed = await camera(page);
			for (let i = 1; i <= 5; i++) {
				await page.mouse.move(bounds.centerX + (dx * i) / 5, bounds.centerY + (dy * i) / 5);
			}
			const dragged = await camera(page);
			await page.mouse.up({ button: 'right' });
			await page.waitForTimeout(200);
			return { pressed, dragged };
		};

		const first = await rotate(90, 30);
		expect(first.pressed.orbitPivot).not.toBeNull();
		// The drag really turned the camera...
		expect(
			Math.hypot(...first.dragged.position.map((v, i) => v - first.pressed.position[i]))
		).toBeGreaterThan(0.1 * distance(first.pressed));
		// ...about the pivot the press anchored: same pivot, same distance.
		expect(first.dragged.orbitPivot).toEqual(first.pressed.orbitPivot);
		expect(distance(first.dragged)).toBeCloseTo(distance(first.pressed), 6);

		// A second rotate from the same place turns about the same spot. The
		// pivot was held under the cursor by the first rotate, so the press
		// lands on it again, to within the 14 px rosette `anchorOrbitPivot`
		// probes with (it takes the hit nearest the camera, not the centre one).
		const second = await rotate(-70, 40);
		const worldPerPx = (2 * viewExtent(second.pressed)) / bounds.height;
		const shift = Math.hypot(
			...second.pressed.orbitPivot.map((v, i) => v - first.pressed.orbitPivot[i])
		);
		expect(shift).toBeLessThan(2 * 14 * worldPerPx);
		expect(second.dragged.orbitPivot).toEqual(second.pressed.orbitPivot);
		expect(distance(second.dragged)).toBeCloseTo(distance(second.pressed), 6);
	});
});
