/**
 * Projected sketch geometry (UI side). See specs/projected_sketch_geometry.md.
 *
 * projectVertex creates a construction Point at the source vertex's position in
 * sketch-plane 2D, records a binding (point id → source) for the engine, and the
 * projection mirrors the Rust SketchPlaneBasis: an in-plane source preserves its
 * distance from the origin, an out-of-plane source drops its normal component.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const bindings = (page) => page.evaluate(() => window.__waffle.getProjectedBindings());
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);

/** A synthetic Vertex GeomRef with a Position selector at a model-space point. */
const vref = (x, y, z) => ({
	kind: { type: 'Vertex' },
	anchor: { type: 'FeatureOutput', feature_id: '00000000-0000-0000-0000-0000000000aa', output_key: { type: 'Main' } },
	selector: { type: 'Position', x, y, z },
});

const projectVertex = (page, ref) =>
	page.evaluate((r) => window.__waffle.projectVertex(r), ref);

const pointById = (ents, id) => ents.find((e) => e.type === 'Point' && e.id === id);

test.describe('projected sketch geometry — projectVertex', () => {
	test('creates a bound construction point; in-plane isometry, out-of-plane drop', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front'); // plane origin [0,0,0], normal [0,0,1]
		await setTool(page, 'project');

		const before = (await entities(page)).filter((e) => e.type === 'Point').length;

		// In-plane source at world (2, 3, 0): distance from origin is preserved.
		const id1 = await projectVertex(page, vref(2, 3, 0));
		expect(id1).not.toBeNull();

		let ents = await entities(page);
		expect(ents.filter((e) => e.type === 'Point').length).toBe(before + 1);
		const p1 = pointById(ents, id1);
		expect(p1.construction).toBe(true);
		expect(Math.hypot(p1.x, p1.y)).toBeCloseTo(Math.hypot(2, 3), 5);

		// A binding was recorded for this point.
		let b = await bindings(page);
		expect(b.length).toBe(1);
		expect(b[0].point_id).toBe(id1);
		expect(b[0].source.kind.type).toBe('Vertex');
		expect(b[0].source.geom_ref.selector.type).toBe('Position');

		// Out-of-plane source at world (2, 3, 9): the normal (z) component is
		// dropped, so it projects to the SAME 2D point as (2, 3, 0).
		const id2 = await projectVertex(page, vref(2, 3, 9));
		ents = await entities(page);
		const p2 = pointById(ents, id2);
		expect(p2.x).toBeCloseTo(p1.x, 6);
		expect(p2.y).toBeCloseTo(p1.y, 6);

		b = await bindings(page);
		expect(b.length).toBe(2);
	});

	test('projected bindings are sent on finish and clear on sketch exit', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');
		await setTool(page, 'project');
		await projectVertex(page, vref(1, 1, 0));
		expect((await bindings(page)).length).toBe(1);

		// Exit the sketch (cancel) — bindings reset for the next sketch.
		await page.evaluate(() => window.__waffle.exitSketch());
		expect((await bindings(page)).length).toBe(0);
	});
});
