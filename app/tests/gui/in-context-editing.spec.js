/**
 * v4 Phase 3d-4 — in-context editing (specs/waffle_v4_document_model.md §2.8,
 * projects/10-assemblies/PLAN.md): from an Assembly tab, a part instance is
 * opened for editing in the assembly's context — the OTHER instances render
 * as ghosts in the edited part's frame, a sketch started on a ghost's face
 * records a scoped plane reference the engine re-derives from the context,
 * the edit propagates to every instance, the context can be updated after
 * the assembly changed, and leaving the context keeps the part editable with
 * a loud note about what it depends on.
 */
import fs from 'fs';
import { test, expect } from './helpers/waffle-test.js';
import { clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import { waitForEntityCount, waitForFeatureCount } from './helpers/state.js';

const CUBE_STEP = fs.readFileSync(new URL('./fixtures/cube.step', import.meta.url), 'utf8');

const near = (a, b, tol = 1e-6) => Math.abs(a - b) <= tol;

/** Part 1 = the 10 mm cube (import + apply), then an Assembly tab, active. */
async function cubePartAndAssembly(page) {
	await page.evaluate((text) => window.__waffle.importStepFromText('cube.step', text), CUBE_STEP);
	await page.waitForFunction(() => window.__waffle.getMeshes().length === 1, { timeout: 30000 });
	await page.locator('[data-testid="import-apply"]').click();
	const partTab = await page.evaluate(() => window.__waffle.getDocumentState().activeTabId);
	await page.locator('[data-testid="tab-add-assembly"]').click();
	await page.waitForFunction(() => window.__waffle.getAssembly() !== null, { timeout: 10000 });
	const asmTab = await page.evaluate(() => window.__waffle.getDocumentState().activeTabId);
	return { partTab, asmTab };
}

test.describe('In-context editing', () => {
	test('edit a part in its assembly: ghosts, a scoped sketch plane, propagation, update, exit', async ({ waffle }) => {
		const page = waffle.page;
		const { partTab, asmTab } = await cubePartAndAssembly(page);
		const a = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'A', fixed: true }), partTab);
		const b = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'B', fixed: true, transform: { translation_m: [0.03, 0, 0], rotation_quat: [0, 0, 0, 1] } }), partTab);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });

		// The panel offers "edit" for a same-document part instance; B opens
		// in context: the Part tab is active, the banner names B and its
		// assembly, and the engine reports one ghost (A).
		await page.locator('[data-testid="asm-instance-edit-context-1"]').click();
		await page.waitForFunction(() => window.__waffle.getEditContext() !== null, { timeout: 30000 });
		const ctx = await page.evaluate(() => window.__waffle.getEditContext());
		expect(ctx.assembly_tab_id).toBe(asmTab);
		expect(ctx.instance_path).toEqual([b]);
		expect(ctx.instance_name).toBe('B');
		expect(ctx.instances.map((i) => i.name)).toEqual(['A']);
		expect(ctx.errors).toEqual([]);
		expect(await page.evaluate(() => window.__waffle.getDocumentState().activeTabId)).toBe(partTab);
		await expect(page.locator('[data-testid="context-banner"]')).toBeVisible();
		await expect(page.locator('[data-testid="context-banner-instance"]')).toHaveText('B');

		// Two meshes: the live part (no instance) and A as a ghost, baked into
		// B's frame (no renderer-side transform), every face ref scoped to A,
		// planar faces carrying the engine's plane. A's top face centroid
		// (5, 5, 10) mm in A's frame is at x − 30 mm in B's.
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });
		let meshes = await page.evaluate(() => window.__waffle.getMeshes());
		const live = meshes.find((m) => !m.context);
		const ghost = meshes.find((m) => m.context);
		expect(live.instancePath).toBeNull();
		expect(ghost.instancePath).toEqual([a]);
		expect(ghost.transform).toBeNull();
		expect(ghost.bodyId.startsWith(a + '/')).toBe(true);
		expect(ghost.faceRanges.length).toBeGreaterThan(0);
		for (const r of ghost.faceRanges) {
			expect(r.geom_ref.scope).toEqual({ tab_id: asmTab, instance_path: [a] });
		}
		expect(live.faceRanges.every((r) => r.geom_ref.scope == null)).toBe(true);
		const topFace = ghost.faceRanges.find((r) => r.plane && near(r.plane.normal[2], 1, 1e-6));
		expect(topFace).toBeTruthy();
		expect(near(topFace.plane.origin[0], -0.025)).toBe(true);
		expect(near(topFace.plane.origin[1], 0.005)).toBe(true);
		expect(near(topFace.plane.origin[2], 0.01)).toBe(true);
		// The ghost is not one of this part's bodies.
		await expect(page.locator('.origin-label', { hasText: 'Bodies (' })).toContainText('Bodies (1)');

		// Sketch on the ghost's top face, draw a rectangle with real pointer
		// events, finish: the sketch's plane is the SCOPED face reference and
		// its snapshot plane is the engine-derived one.
		const started = await page.evaluate((ref) => window.__waffle.enterSketchOnFace(ref), topFace.geom_ref);
		expect(started).toBe(true);
		await page.waitForFunction(() => window.__waffle.getState().sketchMode.active === true, { timeout: 10000 });
		await clickRectangle(page);
		await drawRectangle(page, -60, -40, 60, 40);
		await waitForEntityCount(page, 8, 5000);
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 2, 15000);
		let tree = await page.evaluate(() => window.__waffle.getFeatureTree());
		const sketchFeature = tree.features[1];
		expect(sketchFeature.operation.type).toBe('Sketch');
		const sk = sketchFeature.operation.sketch;
		expect(sk.plane.scope).toEqual({ tab_id: asmTab, instance_path: [a] });
		expect(sk.plane.anchor.feature_id).toBe(topFace.geom_ref.anchor.feature_id);
		expect(near(sk.plane_origin[0], -0.025)).toBe(true);
		expect(near(sk.plane_origin[2], 0.01)).toBe(true);
		expect(near(sk.plane_normal[2], 1)).toBe(true);

		// Extrude it 5 mm: a second body of B on top of A's face — the model
		// now reaches z = 15 mm (the cubes end at 10 mm).
		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('5');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 3, 30000);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 3, { timeout: 30000 });
		const bbox = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
		expect(near(bbox.max[2], 0.015, 1e-4)).toBe(true);
		expect(await page.evaluate(() => window.__waffle.getEditContext()?.instance_name)).toBe('B');

		// Back in the assembly, BOTH instances carry the new body (propagation
		// by recipe), and the context is gone.
		await page.evaluate((id) => window.__waffle.switchTab(id), asmTab);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 4, { timeout: 30000 });
		expect(await page.evaluate(() => window.__waffle.getEditContext())).toBeNull();
		await expect(page.locator('[data-testid="context-banner"]')).toHaveCount(0);

		// Move A 20 mm in y, re-open B in context: the scoped plane follows A.
		await page.evaluate((id) => window.__waffle.updateInstance(id, { transform: { translation_m: [0, 0.02, 0], rotation_quat: [0, 0, 0, 1] } }), a);
		await page.waitForFunction((id) => Math.abs((window.__waffle.getAssembly()?.placements?.[id]?.translation_m?.[1] ?? 0) - 0.02) < 1e-9, a, { timeout: 15000 });
		const opened = await page.evaluate((path) => window.__waffle.openPartInContext(path), [b]);
		expect(opened).toBe(true);
		await page.waitForFunction(() => window.__waffle.getEditContext() !== null, { timeout: 30000 });
		tree = await page.evaluate(() => window.__waffle.getFeatureTree());
		const moved = tree.features[1].operation.sketch;
		expect(near(moved.plane_origin[0], -0.025)).toBe(true);
		expect(near(moved.plane_origin[1], 0.025)).toBe(true);
		expect(near(moved.plane_origin[2], 0.01)).toBe(true);

		// "Update context" re-takes the snapshot (a no-op here) and keeps the
		// context; "Exit context" leaves the part open on its own: no ghost,
		// the plane keeps its last derived value, and the dependency is loud.
		await page.locator('[data-testid="context-banner-update"]').click();
		await page.waitForFunction(() => window.__waffle.getEditContext()?.instance_name === 'B', { timeout: 30000 });
		await page.locator('[data-testid="context-banner-exit"]').click();
		await page.waitForFunction(() => window.__waffle.getEditContext() === null, { timeout: 30000 });
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });
		meshes = await page.evaluate(() => window.__waffle.getMeshes());
		expect(meshes.every((m) => !m.context)).toBe(true);
		tree = await page.evaluate(() => window.__waffle.getFeatureTree());
		expect(near(tree.features[1].operation.sketch.plane_origin[1], 0.025)).toBe(true);
		await page.waitForFunction(() => (window.__waffle.getToasts() || []).some((t) => /open the part in that assembly/.test(t.message)), { timeout: 10000 });

		// The document writes the scoped reference and demands the current
		// reader floor (v5 for `scope`, raised to v6 by `plane_x_axis`).
		const json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(json.version).toBe(6);
		expect(json.min_reader_version).toBe(6);
		const part = json.tabs.find((t) => t.id === partTab);
		expect(part.kind.features.features[1].operation.sketch.plane.scope).toEqual({ tab_id: asmTab, instance_path: [a] });
	});

	test('a linked or unknown instance cannot be opened in context, and the panel only offers same-document parts', async ({ waffle }) => {
		const page = waffle.page;
		const { partTab } = await cubePartAndAssembly(page);
		await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'A', fixed: true }), partTab);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 1, { timeout: 30000 });
		await expect(page.locator('[data-testid="asm-instance-edit-context-0"]')).toBeVisible();
		const opened = await page.evaluate(() => window.__waffle.openPartInContext(['00000000-0000-0000-0000-000000000000']));
		expect(opened).toBe(false);
		expect(await page.evaluate(() => window.__waffle.getEditContext())).toBeNull();
		// Still on the assembly.
		await expect(page.locator('[data-testid="assembly-panel"]')).toBeVisible();
	});
});
