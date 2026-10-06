/**
 * A new sketch's basis is upright in the view it is drawn in.
 *
 * On the Top plane the derived basis (x = X × n = world −Y) made the camera's
 * up vector world +X, so the view cube read "Top" sideways the moment a sketch
 * started — even from an already-aligned Top view (2026-10-06 feedback). New
 * sketches on a ±Z-facing plane now carry world +X as their x axis
 * (`defaultSketchXAxis`, mirrored by the engine's `default_x_axis`), persisted
 * on the feature so the sketch rebuilds in the basis it was drawn in.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch } from './helpers/toolbar.js';
import { drawLine } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';

test.describe('sketch plane orientation', () => {
	test('starting a sketch on the top plane keeps world +Y up, matching the Top view', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await page.waitForTimeout(300);
		const cam = await page.evaluate(() => window.__waffle.getCameraState());
		expect(cam.up.map((v) => Math.round(v * 1e6) / 1e6)).toEqual([0, 1, 0]);
		// Looking down −Z from above, as the Top view does.
		expect(cam.position[2]).toBeGreaterThan(0);
	});

	test('a finished top-plane sketch stores world +X as its x axis', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await drawLine(page, -100, -40, 100, 40);
		await waitForEntityCount(page, 3, 5000);
		await clickFinishSketch(page);
		await page.waitForFunction(() => {
			const tree = window.__waffle.getFeatureTree();
			return tree && tree.features && tree.features.length >= 1;
		}, { timeout: 10000 });
		const sketch = await page.evaluate(() => {
			const f = window.__waffle.getFeatureTree().features.find((x) => x.operation?.type === 'Sketch');
			return f?.operation?.sketch ?? null;
		});
		expect(sketch).not.toBeNull();
		expect(sketch.plane_x_axis).toEqual([1, 0, 0]);
		expect(sketch.plane_normal).toEqual([0, 0, 1]);
	});
});
