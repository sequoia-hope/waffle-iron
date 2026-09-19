/**
 * Sprocket sketch entity (spec `specs/custom_features_and_modeling_roadmap.md`
 * §B3) — create an ISO 606 sprocket via the __waffle API, check its display
 * expansion is points + arcs only, finish the sketch, and extrude it.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch } from './helpers/toolbar.js';
import {
	isSketchActive,
	getEntities,
	waitForFeatureCount,
	hasFeatureOfType,
	collectCrashErrors,
	expectNoAnyCrash,
	hasMeshWithGeometry
} from './helpers/state.js';

/** ISO 08B chain, 12 teeth: 12 × 4 = 48 arcs, 1 + 12 × 7 = 85 points. */
const SPROCKET = { toothCount: 12, pitch: 0.0127, rollerDiameter: 0.00851 };

test.describe('sprocket entity', () => {
	test('create via API, display is arcs only, finish and extrude', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await clickSketch(page);
		expect(await isSketchActive(page)).toBe(true);

		const regId = await page.evaluate((p) => window.__waffle.createSprocket(p), SPROCKET);
		expect(regId).toBeDefined();

		// One compact Sprocket entity in the sketch, its params intact.
		const entities = await getEntities(page);
		const sprockets = entities.filter((e) => e.type === 'Sprocket');
		expect(sprockets).toHaveLength(1);
		expect(sprockets[0].params.toothCount).toBe(12);
		expect(entities.filter((e) => e.type === 'Arc')).toHaveLength(0);

		// The display expansion: 48 profile arcs (+ the construction pitch
		// circle), no splines, every arc with numeric point ids.
		const display = await page.evaluate((id) => window.__waffle.getGearDisplay()[id], regId);
		expect(display.counts.Spline ?? 0).toBe(0);
		const arcs = display.entities.filter((e) => e.type === 'Arc');
		expect(arcs).toHaveLength(48);
		for (const arc of arcs) {
			expect(typeof arc.center_id).toBe('number');
			expect(typeof arc.start_id).toBe('number');
			expect(typeof arc.end_id).toBe('number');
		}
		const circles = display.entities.filter((e) => e.type === 'Circle');
		expect(circles).toHaveLength(1);
		expect(circles[0].construction).toBe(true);
		// The pitch circle is the ISO pitch diameter d = p / sin(π/z), halved.
		expect(display.pitchRadius).toBeCloseTo(0.0127 / Math.sin(Math.PI / 12) / 2, 9);

		// The registry knows the kind, so the gear edit gesture stays off it.
		const kind = await page.evaluate((id) => window.__waffle.getGearRegistry().get(id).kind, regId);
		expect(kind).toBe('Sprocket');

		await clickFinishSketch(page);
		expect(await isSketchActive(page)).toBe(false);
		await waitForFeatureCount(page, 1, 10000);
		expect(await hasFeatureOfType(page, 'Sketch')).toBe(true);

		// The completed sketch renders through the inactive expansion cache.
		await page.waitForFunction(
			() => Object.values(window.__waffle.getInactiveGearDisplay()).some((d) => (d.counts.Arc ?? 0) === 48),
			null,
			{ timeout: 10000 }
		);

		// Extrude it: the kernel builds one cylindrical wall per arc.
		const sketchId = await page.evaluate(
			() => window.__waffle.getFeatureTree().features.find((f) => f.operation.type === 'Sketch').id
		);
		const added = await page.evaluate(
			({ sketchId }) =>
				window.__waffleAgentExecutor.executeTool(
					'feature_add',
					{
						operation: {
							type: 'Extrude',
							params: { sketch_id: sketchId, profile_index: 0, depth: 0.005, symmetric: false, cut: false }
						}
					},
					{ agentName: 'sprocket-test', isPaused: () => false, pause: () => {}, isCancelled: () => false }
				),
			{ sketchId }
		);
		expect(added?.ok ?? added?.error == null, JSON.stringify(added)).toBeTruthy();
		await waitForFeatureCount(page, 2, 20000);
		expect(await hasFeatureOfType(page, 'Extrude')).toBe(true);
		const errors = await page.evaluate(() => [...window.__waffle.getFeatureErrors().entries()]);
		expect(errors).toEqual([]);
		expect(await hasMeshWithGeometry(page)).toBe(true);

		expectNoAnyCrash(crashes);
	});

	test('invalid parameters are refused before anything is added', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await clickSketch(page);

		const refused = await page.evaluate(async () => {
			try {
				await window.__waffle.createSprocket({ toothCount: 3, pitch: 0.0127, rollerDiameter: 0.00851 });
				return null;
			} catch (e) {
				return String(e?.message ?? e);
			}
		});
		expect(refused).toMatch(/tooth_count 3/);
		const entities = await getEntities(page);
		expect(entities.filter((e) => e.type === 'Sprocket')).toHaveLength(0);

		expectNoAnyCrash(crashes);
	});
});
