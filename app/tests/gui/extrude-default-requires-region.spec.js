/**
 * Regression (box2.waffle): the extrude dialog's bare default (`profileIndex 0`,
 * no region clicked) must NOT route a self-intersecting sketch through the legacy
 * whole-profile path — that path can't represent overlapping minimal-face loops
 * and the kernel rejects it (ProfileRepeatedVertex / ProfileLoopsIntersect).
 *
 * The default is resolved against the engine's authoritative arrangement
 * (compute_regions): a sketch whose outer profile equals exactly one region
 * (plain/holed rectangle) extrudes that region; an X-in-square — whose outer
 * square equals NO single region — requires an explicit region pick instead of
 * crashing.
 */
import { test, expect } from './helpers/waffle-test.js';

function doc(sketchEntities, solvedPositions, solvedProfiles) {
	return JSON.stringify({
		format: 'waffle-iron',
		version: 3,
		document: {
			name: 't',
			created: '2026-06-26T00:00:00.000Z',
			modified: '2026-06-26T00:00:00.000Z',
			display_unit: 'mm',
		},
		tabs: [
			{
				id: 'fe19a12d-2f8f-4df7-903e-3548724732d8',
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
										entities: sketchEntities,
										constraints: [],
										solve_status: { type: 'FullyConstrained' },
										solved_positions: solvedPositions,
										solved_profiles: solvedProfiles,
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
}

const pt = (id, x, y) => ({ type: 'Point', id, x, y, construction: false });
const ln = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b, construction: false });

/** X-in-square: outer + inner square + two interior diagonals crossing at origin. */
const X_IN_SQUARE = doc(
	[
		pt(1, -0.025, -0.025), pt(2, 0.025, -0.025), pt(3, 0.025, 0.025), pt(4, -0.025, 0.025),
		ln(5, 1, 2), ln(6, 2, 3), ln(7, 3, 4), ln(8, 4, 1), ln(9, 3, 1),
		pt(11, 0.02, 0.02), pt(12, -0.02, 0.02), pt(13, -0.02, -0.02), pt(14, 0.02, -0.02),
		ln(15, 11, 12), ln(16, 12, 13), ln(17, 13, 14), ln(18, 14, 11), ln(19, 14, 12),
	],
	{ 1: [-0.025, -0.025], 2: [0.025, -0.025], 3: [0.025, 0.025], 4: [-0.025, 0.025],
		11: [0.02, 0.02], 12: [-0.02, 0.02], 13: [-0.02, -0.02], 14: [0.02, -0.02] },
	[{ entity_ids: [5, 6, 7, 8], is_outer: true, vertex_ids: [1, 2, 3, 4] }]
);

/** Plain rectangle: one clean region, default extrude must still work. */
const PLAIN_RECT = doc(
	[
		pt(1, -0.025, -0.025), pt(2, 0.025, -0.025), pt(3, 0.025, 0.025), pt(4, -0.025, 0.025),
		ln(5, 1, 2), ln(6, 2, 3), ln(7, 3, 4), ln(8, 4, 1),
	],
	{ 1: [-0.025, -0.025], 2: [0.025, -0.025], 3: [0.025, 0.025], 4: [-0.025, 0.025] },
	[{ entity_ids: [5, 6, 7, 8], is_outer: true, vertex_ids: [1, 2, 3, 4] }]
);

async function load(waffle, json) {
	await waffle.page.evaluate((j) => window.__waffle.loadProject(j), json);
	await waffle.page.waitForFunction(
		() => window.__waffle.getFeatureTree()?.features?.length >= 1,
		undefined,
		{ timeout: 10000 }
	);
}

test('X-in-square default extrude is blocked and requires a region pick (no crash)', async ({ waffle }) => {
	await load(waffle, X_IN_SQUARE);

	// Open the dialog (bare default profileIndex 0) and apply WITHOUT picking.
	await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
	await waffle.page.evaluate(() => window.__waffle.applyExtrude(0.01, 0, false, {}));

	// The guard must refuse: no Extrude feature is added, no kernel crash.
	await waffle.page.waitForTimeout(500);
	const featureTypes = await waffle.page.evaluate(() =>
		(window.__waffle.getFeatureTree()?.features ?? []).map((f) => f.operation?.type)
	);
	expect(featureTypes.filter((t) => t === 'Extrude').length, 'no extrude was created').toBe(0);
});

test('plain rectangle default extrude still works (single region auto-resolves)', async ({ waffle }) => {
	await load(waffle, PLAIN_RECT);

	await waffle.page.evaluate(() => window.__waffle.showExtrudeDialog());
	await waffle.page.evaluate(() => window.__waffle.applyExtrude(0.01, 0, false, {}));

	await waffle.page.waitForFunction(
		() => (window.__waffle.getFeatureTree()?.features ?? []).some((f) => f.operation?.type === 'Extrude'),
		undefined,
		{ timeout: 10000 }
	);
	const meshes = await waffle.page.evaluate(() => window.__waffle.getMeshes());
	expect(meshes.length, 'plain rectangle extrudes to a solid').toBeGreaterThanOrEqual(1);
});
