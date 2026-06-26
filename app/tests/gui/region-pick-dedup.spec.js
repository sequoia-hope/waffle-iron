/**
 * Regression (box2.waffle): the four internal triangles of an X-in-square all
 * meet at the origin. Each carries the origin as a vertex and all have equal
 * area, so a sub-region dedup keyed on (first outer vertex + area) collides —
 * two opposite triangles produce the SAME key and the second click is silently
 * dropped (hover highlights it, but it never joins the extrude). The key must be
 * the area-weighted centroid, which is distinct per region.
 */
import { test, expect } from './helpers/waffle-test.js';

const pt = (id, x, y) => ({ type: 'Point', id, x, y, construction: false });
const ln = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b, construction: false });

const SKETCH_ID = '0412204d-3738-46f4-aeef-a64996944886';

const X_IN_SQUARE = JSON.stringify({
	format: 'waffle-iron',
	version: 3,
	document: { name: 't', created: '2026-06-26T00:00:00.000Z', modified: '2026-06-26T00:00:00.000Z', display_unit: 'mm' },
	tabs: [
		{
			id: 'fe19a12d-2f8f-4df7-903e-3548724732d8',
			name: 'Part 1',
			kind: {
				type: 'Part',
				features: {
					features: [
						{
							id: SKETCH_ID,
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
										pt(1, -0.025, -0.025), pt(2, 0.025, -0.025), pt(3, 0.025, 0.025), pt(4, -0.025, 0.025),
										ln(5, 1, 2), ln(6, 2, 3), ln(7, 3, 4), ln(8, 4, 1), ln(9, 3, 1),
										pt(11, 0.02, 0.02), pt(12, -0.02, 0.02), pt(13, -0.02, -0.02), pt(14, 0.02, -0.02),
										ln(15, 11, 12), ln(16, 12, 13), ln(17, 13, 14), ln(18, 14, 11), ln(19, 14, 12),
									],
									constraints: [],
									solve_status: { type: 'FullyConstrained' },
									solved_positions: {
										1: [-0.025, -0.025], 2: [0.025, -0.025], 3: [0.025, 0.025], 4: [-0.025, 0.025],
										11: [0.02, 0.02], 12: [-0.02, 0.02], 13: [-0.02, -0.02], 14: [0.02, -0.02],
									},
									solved_profiles: [{ entity_ids: [5, 6, 7, 8], is_outer: true, vertex_ids: [1, 2, 3, 4] }],
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
	active_tab: 'fe19a12d-2f8f-4df7-903e-3548724732d8',
});

test('all four origin-touching triangles can be added to the extrude (centroid dedup)', async ({ waffle }) => {
	await waffle.page.evaluate((j) => window.__waffle.loadProject(j), X_IN_SQUARE);
	await waffle.page.waitForFunction(() => window.__waffle.getFeatureTree()?.features?.length >= 1, undefined, { timeout: 10000 });

	// Open the dialog and compute regions (entering pick mode triggers it).
	await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
	await waffle.page.evaluate(() => window.__waffle.setProfilePickMode(true));
	await waffle.page.waitForFunction(
		(id) => (window.__waffle.getSketchRegions(id)?.length ?? 0) >= 6,
		SKETCH_ID,
		{ timeout: 10000 }
	);

	const added = await waffle.page.evaluate((id) => {
		// Clear the dialog's auto-populated default.
		let ex = window.__waffle.getExtrudeRegions();
		for (let i = ex.length - 1; i >= 0; i--) window.__waffle.removeExtrudeRegion(i);

		// The four inner triangles (each an eighth of the inner square = 0.0004),
		// all meeting at the origin. Add each as a sub-region pick.
		const regions = window.__waffle.getSketchRegions(id);
		const triangles = regions.filter((r) => Math.abs((r.area ?? 0) - 0.0004) < 1e-5);
		for (const r of triangles) window.__waffle.addExtrudeRegion(id, 'Sketch', r._index, r);

		return {
			triangleCount: triangles.length,
			selected: window.__waffle.getExtrudeRegions().filter((r) => r.region && r.region.profile_entity_ids == null).length,
		};
	}, SKETCH_ID);

	expect(added.triangleCount, 'X-in-square has 4 inner triangles').toBe(4);
	expect(added.selected, 'all four triangles are added (no key collision)').toBe(4);
});
