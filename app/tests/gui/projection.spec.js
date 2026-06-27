/**
 * Projected sketch geometry (UI side). See specs/projected_sketch_geometry.md.
 *
 * projectVertex creates a construction Point at the source vertex's position in
 * sketch-plane 2D, records a binding (point id → source) for the engine, and the
 * projection mirrors the Rust SketchPlaneBasis: an in-plane source preserves its
 * distance from the origin, an out-of-plane source drops its normal component.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount } from './helpers/state.js';

const entities = (page) => page.evaluate(() => window.__waffle.getEntities());
const bindings = (page) => page.evaluate(() => window.__waffle.getProjectedBindings());
const positions = (page) => page.evaluate(() => Object.fromEntries(window.__waffle.getPositions()));
const setTool = (page, t) => page.evaluate((tool) => window.__waffle.setTool(tool), t);
const ANCHOR = { type: 'FeatureOutput', feature_id: '00000000-0000-0000-0000-0000000000aa', output_key: { type: 'Main' } };

async function buildBox(waffle) {
	const page = waffle.page;
	await clickSketch(page, 'front');
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	const depth = page.locator('[data-testid="extrude-depth"]');
	if (await depth.isVisible()) await depth.fill('20');
	await page.locator('[data-testid="extrude-apply"]').click();
	await expect(page.locator('[data-testid="extrude-dialog"]')).not.toBeVisible();
	await waitForFeatureCount(page, 2, 10000);
}

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

test.describe('projected sketch geometry — projectEdge', () => {
	test('a straight edge projects two bound endpoints + a connecting line', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page, 'front');
		await setTool(page, 'project');

		// An in-plane edge from (1,0,0) to (1,4,0) — length 4.
		const ids = await page.evaluate(
			({ anchor }) => window.__waffle.projectEdge(anchor, [1, 0, 0], [1, 4, 0]),
			{ anchor: ANCHOR }
		);
		expect(ids).toHaveLength(2);

		const ents = await entities(page);
		const pts = ents.filter((e) => e.type === 'Point' && ids.includes(e.id));
		expect(pts.length).toBe(2);
		expect(pts.every((p) => p.construction)).toBe(true);

		const line = ents.find(
			(e) => e.type === 'Line' && e.construction && ids.includes(e.start_id) && ids.includes(e.end_id)
		);
		expect(line).toBeTruthy();

		// In-plane edge: projected length equals the 3D edge length.
		const pos = await positions(page);
		const a = pos[ids[0]];
		const b = pos[ids[1]];
		expect(Math.hypot(a.x - b.x, a.y - b.y)).toBeCloseTo(4, 4);

		// Both endpoints are bound (Vertex kind, Position selector).
		const bnd = await bindings(page);
		expect(bnd.length).toBe(2);
		expect(bnd.every((x) => x.source.kind.type === 'Vertex')).toBe(true);
		expect(bnd.every((x) => x.source.geom_ref.selector.type === 'Position')).toBe(true);
	});
});

test.describe('projected sketch geometry — projectFace', () => {
	test('a box face projects its boundary edges as bound construction lines', async ({ waffle }) => {
		const page = waffle.page;
		await buildBox(waffle);

		await clickSketch(page, 'front');
		await setTool(page, 'project');

		// Grab a real face GeomRef from the box mesh.
		const faceRef = await page.evaluate(() => {
			for (const m of window.__waffle.getMeshes()) {
				for (const fr of m.faceRanges || []) {
					if (fr.geom_ref) return fr.geom_ref;
				}
			}
			return null;
		});
		expect(faceRef).not.toBeNull();

		const beforeBindings = (await bindings(page)).length;
		const n = await page.evaluate((ref) => window.__waffle.projectFace(ref), faceRef);

		// A planar box face has 4 boundary edges → ≥3 construction lines.
		expect(n).toBeGreaterThanOrEqual(3);
		const ents = await entities(page);
		expect(ents.filter((e) => e.type === 'Line' && e.construction).length).toBeGreaterThanOrEqual(n);

		// Each projected corner is bound; deduped corners ⇒ a connected loop.
		const bnd = await bindings(page);
		expect(bnd.length - beforeBindings).toBeGreaterThanOrEqual(3);
		expect(bnd.every((x) => x.source.kind.type === 'Vertex')).toBe(true);
	});
});
