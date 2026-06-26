/**
 * Regression (BOXPATCH): sketch region extraction must use the solver output
 * (`solved_positions`), the engine's authoritative geometry, NOT the raw drawn
 * `entity.x/y` scratch (an A2.1/A5.2 violation). The repro is box.waffle: two
 * nested squares the solver centers on the origin, drawn from rough off-center
 * raw coordinates where the "inner" square is actually LARGER than the outer.
 *
 * Fed raw coords, `compute_regions` produces a geometrically wrong arrangement
 * (overlapping wrong-size squares). Fed solved coords, it produces the correct
 * X-in-square: four inner triangles + two frame pieces, tiling the 0.05 square.
 *
 * This loads a fixture whose raw coords differ sharply from `solved_positions`
 * and asserts the regions reflect the SOLVED geometry — failing on the old
 * raw-coordinate code path.
 */
import { test, expect } from './helpers/waffle-test.js';

/** Document with raw entity coords (box.waffle's actual pre-solve scratch) that
 *  differ sharply from the clean solved X-in-square. */
const FIXTURE = {
	format: 'waffle-iron',
	version: 3,
	document: {
		name: 'xinsquare',
		created: '2026-06-26T00:00:00.000Z',
		modified: '2026-06-26T00:00:00.000Z',
		display_unit: 'mm',
	},
	tabs: [
		{
			id: '0d418f17-0c9f-4cea-b879-bb4015967284',
			name: 'Part 1',
			kind: {
				type: 'Part',
				features: {
					features: [
						{
							id: '0412204d-3738-46f4-aeef-a64996944886',
							name: 'Sketch',
							operation: {
								type: 'Sketch',
								sketch: {
									id: '887d8844-b0a1-4d51-80af-40197445ae7c',
									plane: {
										kind: { type: 'Face' },
										anchor: { type: 'Datum', datum_id: '36547226-8ec6-4ee6-b7fd-9814d3e3d362' },
										selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
										policy: { type: 'BestEffort' },
									},
									plane_origin: [0, 0, 0],
									plane_normal: [0, 1, 0],
									entities: [
										// RAW coords: off-center, and "inner" (11-14) LARGER than
										// "outer" (1-4) — exactly box.waffle's pre-solve scratch.
										{ type: 'Point', id: 1, x: -0.0125, y: -0.0102 },
										{ type: 'Point', id: 2, x: 0.0146, y: -0.0102 },
										{ type: 'Point', id: 3, x: 0.0146, y: 0.0152 },
										{ type: 'Point', id: 4, x: -0.0125, y: 0.0152 },
										{ type: 'Line', id: 5, start_id: 1, end_id: 2 },
										{ type: 'Line', id: 6, start_id: 2, end_id: 3 },
										{ type: 'Line', id: 7, start_id: 3, end_id: 4 },
										{ type: 'Line', id: 8, start_id: 4, end_id: 1 },
										{ type: 'Line', id: 9, start_id: 3, end_id: 1 },
										{ type: 'Point', id: 11, x: 0.0202, y: 0.0202 },
										{ type: 'Point', id: 12, x: -0.016, y: 0.0202 },
										{ type: 'Point', id: 13, x: -0.016, y: -0.016 },
										{ type: 'Point', id: 14, x: 0.0202, y: -0.016 },
										{ type: 'Line', id: 15, start_id: 11, end_id: 12 },
										{ type: 'Line', id: 16, start_id: 12, end_id: 13 },
										{ type: 'Line', id: 17, start_id: 13, end_id: 14 },
										{ type: 'Line', id: 18, start_id: 14, end_id: 11 },
										{ type: 'Line', id: 19, start_id: 14, end_id: 12 },
									],
									constraints: [],
									solve_status: { type: 'FullyConstrained' },
									// SOLVED: clean concentric X-in-square (outer ±0.025, inner ±0.02).
									solved_positions: {
										1: [-0.025, -0.025],
										2: [0.025, -0.025],
										3: [0.025, 0.025],
										4: [-0.025, 0.025],
										11: [0.02, 0.02],
										12: [-0.02, 0.02],
										13: [-0.02, -0.02],
										14: [0.02, -0.02],
									},
								},
							},
							suppressed: false,
							references: [],
						},
					],
					active_index: null,
				},
			},
		},
	],
	active_tab: '0d418f17-0c9f-4cea-b879-bb4015967284',
};

const SKETCH_ID = '0412204d-3738-46f4-aeef-a64996944886';

function polygonAreaAbs(outer) {
	let a = 0;
	for (let i = 0; i < outer.length; i++) {
		const [x1, y1] = outer[i];
		const [x2, y2] = outer[(i + 1) % outer.length];
		a += x1 * y2 - x2 * y1;
	}
	return Math.abs(a / 2);
}

test('sketch regions are computed from solved_positions, not raw drawn coords', async ({ waffle }) => {
	// Load the fixture and enter profile-pick mode (which triggers region compute).
	await waffle.page.evaluate((doc) => window.__waffle.loadProject(JSON.stringify(doc)), FIXTURE);
	await waffle.page.waitForFunction(
		(id) => window.__waffle.getFeatureTree()?.features?.some((f) => f.id === id),
		SKETCH_ID,
		{ timeout: 10000 }
	);
	await waffle.page.evaluate(() => window.__waffle.setProfilePickMode(true));

	// Wait for the async region computation to populate.
	await waffle.page.waitForFunction(
		(id) => {
			const r = window.__waffle.getSketchRegions(id);
			return Array.isArray(r) && r.length > 0;
		},
		SKETCH_ID,
		{ timeout: 10000 }
	);

	const regions = await waffle.page.evaluate((id) => window.__waffle.getSketchRegions(id), SKETCH_ID);

	// The SOLVED arrangement: 4 inner triangles + 2 frame pieces. Raw coords
	// (inner square larger than outer, offset) would not produce this.
	expect(regions.length, 'X-in-square solved arrangement = 6 regions').toBe(6);

	// Areas tile the SOLVED outer square (0.05 * 0.05 = 0.0025), not the raw one.
	const total = regions.reduce((s, r) => s + polygonAreaAbs(r.outer), 0);
	expect(Math.abs(total - 0.0025)).toBeLessThan(1e-6);

	// Four triangular inner quadrants, each 0.0004 (eighth of the inner 0.04 square).
	const triangles = regions.filter((r) => r.outer.length === 3);
	expect(triangles.length, 'inner square diagonals yield 4 triangles').toBe(4);
	for (const t of triangles) {
		expect(Math.abs(polygonAreaAbs(t.outer) - 0.0004)).toBeLessThan(1e-6);
	}
});
