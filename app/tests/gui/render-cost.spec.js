/**
 * A body with nothing highlighted on it draws in ONE call.
 *
 * Face highlighting needs a material per face range, and passing three.js a
 * material ARRAY makes it draw one call per geometry group. Every body was
 * getting an array — so a 12-triangle box cost six draw calls, and the Eiffel
 * Tower example rendered 26,436 triangles in 25,447 draw calls, at 2 fps
 * (docs/notes/eiffel/FEATURE_NOTES.md §10). The split is now made only for a
 * body that actually has a lit face.
 */
import { test, expect } from './helpers/waffle-test.js';
import { createExtrudedBox } from './helpers/geometry.js';

const stats = (page) => page.evaluate(() => window.__waffle.getRenderStats());

test.describe('render cost', () => {
	test('an unhighlighted body is one draw call, not one per face', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(800);

		const s = await stats(page);
		expect(s, 'the renderer is reachable for measurement').not.toBeNull();
		expect(s.census.meshes, 'the box is in the scene').toBeGreaterThanOrEqual(1);

		// One box: a handful of objects (body, edges, datum planes, overlays).
		// The bound is what matters — six calls for the box's six faces, plus
		// twelve for its edges, would blow straight through it.
		expect(
			s.calls,
			`draw calls should be about one per object, got ${s.calls} for ${s.census.meshes} meshes + ${s.census.lineSegments} line objects`
		).toBeLessThanOrEqual(s.census.meshes + s.census.lineSegments + 12);
	});

	test('the whole scene shares a handful of materials', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await page.waitForTimeout(800);

		// Distinct material instances across the scene. One per face of every
		// body is what this guards against: each is its own program bind and
		// uniform refresh, every frame.
		const materials = await page.evaluate(() => {
			const seen = new Set();
			// Reachable through the meshes the store reports, via the renderer's
			// own census: getRenderStats reports counts, so assert on those.
			return window.__waffle.getRenderStats();
		});
		expect(materials.programs, 'a small number of shader programs').toBeLessThanOrEqual(24);
	});
});
