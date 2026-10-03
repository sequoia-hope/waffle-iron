/**
 * N2 of `specs/agent_mechanical_design.md` §5.3 item 3, in the real app: a
 * sketch drawn on a model face re-resolves that face on every rebuild, and
 * says so on its own feature-tree row.
 *
 * The model is the one §5.3 names — a plate, a boss on it, a sketch on the
 * boss's top face — and the two things that can then happen to that face:
 *
 * 1. the boss is DELETED, so the face is gone: the sketch refuses, its row
 *    carries the error glyph naming the face that went missing, and the
 *    sketch does NOT land on the plate's top face, which is still right
 *    there and is what a `BestEffort` rebind would have found;
 * 2. the boss is made SHORTER, so the face moved: the sketch still builds and
 *    its row carries the WARNING glyph (N2's minimal UI — a glyph and a
 *    tooltip, reusing the error indicator's affordance).
 *
 * Why this needs a GUI test and not only the Rust ones: the face reference is
 * recorded by `beginSketchPlaneRef` on the page, from the face the user
 * PICKED in the viewport. A Rust test can pin the engine's half of that
 * contract; only this can show that a user's pick reaches it at all.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureTree,
	getMeshes,
	waitForMeshWithGeometry,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

/** Select a face ref programmatically (a stand-in for the viewport click). */
async function selectFaceRef(page, ref) {
	await page.evaluate((r) => window.__waffle.selectRef(r), ref);
	await page.waitForTimeout(200);
}

/**
 * The +Z-facing face with the highest origin — the top of whatever is
 * currently there, which is the plate top before the boss and the boss top
 * after it.
 *
 * The plane comes from `computeFacePlane`, the page's own resolver: a face
 * range's `geom_ref` carries a fingerprint only when the face is ROLELESS,
 * and an extrude's end cap has a role, so there is no signature on the range
 * to read. This is the same call the sketch entry makes to place the plane.
 */
async function topFaceRef(page) {
	return page.evaluate(() => {
		let best = null;
		for (const mesh of window.__waffle.getMeshes()) {
			for (const range of mesh.faceRanges ?? []) {
				if (!range.geom_ref) continue;
				const plane = window.__waffle.computeFacePlane(range.geom_ref);
				if (!plane || !plane.normal || !plane.origin) continue;
				const n = plane.normal;
				const o = plane.origin;
				const nz = Array.isArray(n) ? n[2] : n.z;
				const oz = Array.isArray(o) ? o[2] : o.z;
				if (!(nz > 0.9)) continue;
				if (!best || oz > best.z) best = { z: oz, ref: range.geom_ref };
			}
		}
		return best ? { ref: best.ref, z: best.z } : null;
	});
}

/** A sketched rectangle extruded `depth`, on whatever plane/face is selected. */
async function sketchRectAndExtrude(waffle, { coords, depth, featuresBefore, label }) {
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, ...coords);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
	await waitForFeatureCount(waffle.page, featuresBefore + 1, 10000);
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill(depth);
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(waffle.page, featuresBefore + 2, 10000);
	if ((await getFeatureTree(waffle.page)).features.length !== featuresBefore + 2) {
		await waffle.dumpState(`n2-${label}-failed`);
	}
}

/**
 * Plate (0), its extrude (1), boss sketch (2), boss extrude (3), then a sketch
 * on the BOSS's top face (4) with its own extrude (5). Returns the z of the
 * boss top, so a test can tell "moved" from "gone".
 */
async function plateBossAndSketchOnTop(waffle) {
	await clickSketch(waffle.page, 'front');
	await sketchRectAndExtrude(waffle, {
		coords: [-80, -60, 80, 60],
		depth: '10',
		featuresBefore: 0,
		label: 'plate',
	});

	await waitForMeshWithGeometry(waffle.page);
	const plateTop = await topFaceRef(waffle.page);
	expect(plateTop, 'the plate has an upward face to sketch on').toBeTruthy();
	await selectFaceRef(waffle.page, plateTop.ref);
	await clickSketch(waffle.page);
	await sketchRectAndExtrude(waffle, {
		coords: [-40, -30, 40, 30],
		depth: '8',
		featuresBefore: 2,
		label: 'boss',
	});

	await waitForMeshWithGeometry(waffle.page);
	const bossTop = await topFaceRef(waffle.page);
	expect(bossTop, 'the boss has an upward face').toBeTruthy();
	expect(
		bossTop.z,
		'the boss top is above the plate top, so they are different faces'
	).toBeGreaterThan(plateTop.z + 1e-6);
	await selectFaceRef(waffle.page, bossTop.ref);
	await clickSketch(waffle.page);
	await sketchRectAndExtrude(waffle, {
		coords: [-20, -15, 20, 15],
		depth: '4',
		featuresBefore: 4,
		label: 'on-boss',
	});

	return { plateTopZ: plateTop.z, bossTopZ: bossTop.z };
}

/**
 * The feature-tree index of the feature with this id. BY ID, never by name:
 * the app's derived names are not unique (a six-feature part here is
 * `Sketch, Extrude, Sketch, Extrude, Sketch, Extrude`).
 */
async function indexOfId(page, id) {
	const tree = await getFeatureTree(page);
	return tree.features.findIndex((f) => f.id === id);
}

/** Delete the feature at `index` through the tree's own context menu. */
async function deleteFeatureAt(page, index) {
	await page.locator(`[data-testid="feature-item-${index}"]`).click({ button: 'right' });
	await page.locator('[data-testid="ft-ctx-delete"]').click();
	await page.waitForTimeout(600);
}

test.describe('N2: a sketch on a model face is loud about that face', () => {
	test('the boss is deleted: the sketch on its top refuses and lands on no other face', async ({
		waffle,
	}) => {
		const crashes = collectCrashErrors(waffle.page);
		const { plateTopZ } = await plateBossAndSketchOnTop(waffle);

		// The sketch on the boss top, by id, before anything moves.
		const treeBefore = await getFeatureTree(waffle.page);
		expect(treeBefore.features).toHaveLength(6);
		const sketchId = treeBefore.features[4].id;
		const bossId = treeBefore.features[3].id;

		// Take the boss away. Its top face is gone; the plate's top is not.
		await deleteFeatureAt(waffle.page, 3);
		const treeAfter = await getFeatureTree(waffle.page);
		expect(
			treeAfter.features.map((f) => f.id),
			'only the boss extrude was deleted'
		).not.toContain(bossId);
		const sketchIndex = await indexOfId(waffle.page, sketchId);
		expect(sketchIndex, 'the sketch is still in the tree').toBeGreaterThanOrEqual(0);

		// Its row carries the glyph, and the tooltip names the face that went
		// missing — the whole point of recording the signature.
		const glyph = waffle.page.locator(`[data-testid="feature-error-${sketchIndex}"]`);
		await expect(glyph).toBeVisible();
		const tooltip = await glyph.getAttribute('title');
		expect(tooltip).toContain('the face this sketch is drawn on is gone');
		expect(tooltip, 'and what the face WAS').toContain('It was a planar face');

		// And nothing was rebound: the sketch produced no geometry, so no body
		// sits on the plate's top face where a `BestEffort` match would have
		// put one. The only remaining body is the plate.
		const meshes = await getMeshes(waffle.page);
		const withGeometry = meshes.filter((m) => m.triangleCount > 0);
		expect(withGeometry).toHaveLength(1);
		const remainingTop = await topFaceRef(waffle.page);
		expect(remainingTop, 'the plate still has its top').toBeTruthy();
		expect(
			remainingTop.z,
			'nothing stands above the plate top any more'
		).toBeLessThan(plateTopZ + 1e-6);

		expectNoAnyCrash(crashes);
	});

	test('the boss is made shorter: the sketch on its top warns on its own row', async ({
		waffle,
	}) => {
		const crashes = collectCrashErrors(waffle.page);
		await plateBossAndSketchOnTop(waffle);
		const sketchId = (await getFeatureTree(waffle.page)).features[4].id;

		// Edit the boss extrude: 8 mm becomes 3. The top face still exists and
		// still carries its persistent id; it has just moved 5 mm down.
		await waffle.page
			.locator('[data-testid="feature-item-3"]')
			.click({ button: 'right' });
		await waffle.page.locator('[data-testid="ft-ctx-edit-feature"]').click();
		await waffle.page.locator('[data-testid="extrude-depth"]').fill('3');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();
		await waffle.page.waitForTimeout(1200);

		const sketchIndex = await indexOfId(waffle.page, sketchId);
		expect(sketchIndex).toBeGreaterThanOrEqual(0);
		// No refusal: the face is still there.
		await expect(
			waffle.page.locator(`[data-testid="feature-error-${sketchIndex}"]`)
		).toHaveCount(0);
		// But the row says the face moved — the N2 glyph.
		const warning = waffle.page.locator(`[data-testid="feature-warning-${sketchIndex}"]`);
		await expect(warning).toBeVisible();
		const tooltip = await warning.getAttribute('title');
		expect(tooltip).toContain('the face this sketch is drawn on has moved');
		expect(tooltip, 'and what the engine did about it').toContain(
			'keeps the frame it was solved in'
		);

		expectNoAnyCrash(crashes);
	});
});
