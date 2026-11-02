/**
 * Extrude alignment tests — the body the engine builds lies exactly where the
 * sketch was drawn: Rust's `SketchPlaneBasis` (via `sketch_basis`) and JS's
 * `buildSketchPlane()` map sketch (u, v) to the same world points.
 *
 * Expectations are derived from the frame the app itself drew in
 * (`getState().sketchMode.xAxis`, persisted as `plane_x_axis`), never from a
 * hardcoded basis: a frame-convention change must not read as misalignment,
 * and a Rust/JS disagreement must not hide behind one.
 *
 * Convention pinned separately: since c9311cc9 (2026-10-06) a new sketch on a
 * ±Z plane carries plane_x_axis = world +X (Top draws upright), so on XY
 * sketch +u → world +X and +v → n × X = world +Y. The original 2026 bug
 * (tangent_x_from_normal disagreeing with buildSketchPlane) showed as the
 * body landing a quarter turn or mirror off the drawn rectangle.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/**
 * Helper: create a sketch with a rectangle using API entities for precise coordinates.
 * Uses addSketchEntity to place points at exact sketch-space positions.
 */
async function createPreciseRectangleSketch(page, { x1, y1, x2, y2 }) {
	// Enter sketch mode on XY plane (normal=[0,0,1])
	// Started, not awaited over the protocol (CDP "Promise was collected"); the wait is the sync.
	await page.evaluate(() => void window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.evaluate(() => window.__waffle.setTool('select'));
	await page.waitForTimeout(200);

	// Add rectangle entities at precise sketch coordinates via API
	await page.evaluate(({ x1, y1, x2, y2 }) => {
		const w = window.__waffle;
		// Four corners as points
		w.addSketchEntity({ id: 1, type: 'Point', x: x1, y: y1 });
		w.addSketchEntity({ id: 2, type: 'Point', x: x2, y: y1 });
		w.addSketchEntity({ id: 3, type: 'Point', x: x2, y: y2 });
		w.addSketchEntity({ id: 4, type: 'Point', x: x1, y: y2 });
		// Four lines forming closed rectangle
		w.addSketchEntity({ id: 5, type: 'Line', start_id: 1, end_id: 2 });
		w.addSketchEntity({ id: 6, type: 'Line', start_id: 2, end_id: 3 });
		w.addSketchEntity({ id: 7, type: 'Line', start_id: 3, end_id: 4 });
		w.addSketchEntity({ id: 8, type: 'Line', start_id: 4, end_id: 1 });
	}, { x1, y1, x2, y2 });

	await waitForEntityCount(page, 8, 3000);

	// The frame the sketch is drawn in — what the engine must build in.
	const frame = await page.evaluate(() => {
		const m = window.__waffle.getState().sketchMode;
		return { origin: m.origin, normal: m.normal, xAxis: m.xAxis };
	});

	// Finish sketch
	await page.evaluate(() => void window.__waffle.finishSketch());
	await waitForFeatureCount(page, 1, 10000);
	return frame;
}

const cross = (a, b) => [
	a[1] * b[2] - a[2] * b[1],
	a[2] * b[0] - a[0] * b[2],
	a[0] * b[1] - a[1] * b[0],
];

/**
 * World-space bbox of a sketch rectangle extruded `depth` along the normal,
 * through the sketch's own frame (y = n × x, as buildSketchPlane builds it).
 */
function expectedBox({ origin, normal, xAxis }, { x1, y1, x2, y2 }, depth) {
	const yAxis = cross(normal, xAxis);
	const pts = [];
	for (const u of [x1, x2]) for (const v of [y1, y2]) for (const w of [0, depth]) {
		pts.push([0, 1, 2].map(i => origin[i] + u * xAxis[i] + v * yAxis[i] + w * normal[i]));
	}
	const min = [0, 1, 2].map(i => Math.min(...pts.map(p => p[i])));
	const max = [0, 1, 2].map(i => Math.max(...pts.map(p => p[i])));
	return { min, max };
}

function expectBoxClose(bbox, want, digits) {
	for (let i = 0; i < 3; i++) {
		expect(bbox.min[i], `min[${i}]`).toBeCloseTo(want.min[i], digits);
		expect(bbox.max[i], `max[${i}]`).toBeCloseTo(want.max[i], digits);
	}
}

async function extrude(page, depth) {
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForFunction(
		() => window.__waffle?.getExtrudeDialogState() !== null,
		{ timeout: 5000 }
	);
	await page.evaluate((d) => void window.__waffle.applyExtrude(d, 0, false), depth);
	await waitForMeshWithGeometry(page, 15000);
}

test.describe('extrude alignment with sketch coordinate frame', () => {
	test('new XY sketch draws in the upright Top frame (+u = world +X)', async ({ waffle }) => {
		// Pins the convention (c9311cc9): a ±Z plane's new sketch carries
		// plane_x_axis = +X. The frame-derived tests below would follow a
		// convention change silently; this one names it.
		const rect = { x1: 5, y1: 5, x2: 15, y2: 15 };
		const frame = await createPreciseRectangleSketch(waffle.page, rect);
		expect(frame.xAxis).toEqual([1, 0, 0]);
		expect(cross(frame.normal, frame.xAxis)).toEqual([0, 1, 0]);

		await extrude(waffle.page, 10);
		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();
		// Literal numbers for the pinned frame: worldX = u, worldY = v, worldZ = depth.
		expectBoxClose(bbox, { min: [5, 5, 0], max: [15, 15, 10] }, 3);
	});

	test('XY extrude of a non-square rectangle lands where it was drawn', async ({ waffle }) => {
		// 20×30 is not square: a quarter-turn Rust/JS frame mismatch swaps the
		// sizes, a mirror flips the sign of a coordinate — both fail here.
		const rect = { x1: 0, y1: 0, x2: 20, y2: 30 };
		const frame = await createPreciseRectangleSketch(waffle.page, rect);
		await extrude(waffle.page, 5);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();
		expectBoxClose(bbox, expectedBox(frame, rect, 5), 3);
	});

	test('small off-origin square on XY plane extrudes lined up (3mm, offset)', async ({ waffle }) => {
		// The user's reported scenario: a SMALL (3×3mm) square on a primary plane,
		// positioned in-plane AWAY from the origin (near corner 5mm out), at real
		// model scale (metres). It must extrude exactly where the sketch is — not
		// collapse toward the sketch origin and not malform at small scale.
		const rect = { x1: 0.005, y1: 0.005, x2: 0.008, y2: 0.008 };
		const frame = await createPreciseRectangleSketch(waffle.page, rect);
		await extrude(waffle.page, 0.002);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();
		// A 3×3×2mm box sitting OFF the origin, where the square was drawn.
		expect(bbox.size[0]).toBeCloseTo(0.003, 4);
		expect(bbox.size[1]).toBeCloseTo(0.003, 4);
		expect(bbox.size[2]).toBeCloseTo(0.002, 4);
		expectBoxClose(bbox, expectedBox(frame, rect, 0.002), 4);
	});

	test('GUI-drawn rectangle on XY extrudes where it was drawn', async ({ waffle }) => {
		// Real pointer drawing (not API) through the full pipeline: the body must
		// cover the rectangle exactly as the sketcher placed it.
		await clickSketch(waffle.page);
		await clickRectangle(waffle.page);

		// Draw a large rectangle offset to the right side of canvas
		await drawRectangle(waffle.page, 20, 20, 120, 80);

		try {
			await waitForEntityCount(waffle.page, 8, 5000);
		} catch {
			await waffle.dumpState('alignment-gui-draw-failed');
			throw new Error('Failed to draw rectangle for alignment test');
		}

		// Where the drawn corners sit, in the frame they were drawn in.
		const drawn = await waffle.page.evaluate(() => {
			const w = window.__waffle;
			const m = w.getState().sketchMode;
			const pos = w.getPositions();
			const pts = w.getEntities()
				.filter((e) => e.type === 'Point')
				.map((e) => pos.get(e.id) ?? { x: e.x, y: e.y });
			return { frame: { origin: m.origin, normal: m.normal, xAxis: m.xAxis }, pts };
		});
		expect(drawn.pts.length).toBeGreaterThanOrEqual(4);
		const xs = drawn.pts.map((p) => p.x);
		const ys = drawn.pts.map((p) => p.y);
		const rect = { x1: Math.min(...xs), y1: Math.min(...ys), x2: Math.max(...xs), y2: Math.max(...ys) };
		// A real, non-degenerate, non-square rectangle (a quarter turn must show).
		expect(rect.x2 - rect.x1).toBeGreaterThan(0);
		expect(rect.y2 - rect.y1).toBeGreaterThan(0);

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);
		await extrude(waffle.page, 10);

		const bbox = await waffle.page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(bbox).not.toBeNull();
		expectBoxClose(bbox, expectedBox(drawn.frame, rect, 10), 3);
	});
});
