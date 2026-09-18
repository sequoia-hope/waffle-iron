/**
 * v4 Phase 3 — assemblies (specs/waffle_v4_document_model.md §9 Phase 3,
 * projects/10-assemblies/PLAN.md): an Assembly tab instantiates this
 * document's Part tabs, the engine renders every instance's bodies with its
 * placement, Fastened mates place instances by rigid-transform composition
 * (with derived placements persisted), the document round-trips through the
 * writer and reopens on its assembly tab, and the panel drives the same edits.
 */
import fs from 'fs';
import { test, expect } from './helpers/waffle-test.js';
import { getDocumentFromDB } from './helpers/waffle-test.js';

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
	await expect(page.locator('[data-testid="assembly-panel"]')).toBeVisible();
	return partTab;
}

/**
 * A Part tab holding one cylinder — radius 10, height 20, axis +Z through the
 * origin, in the units the sketch API takes (unconverted, as
 * `helpers/geometry.js` builds its box) — then an Assembly tab with one
 * instance of it. An imported STEP body would NOT do here: the importer keeps
 * no analytic surface parameters, so its faces carry no axis (spec §2.2).
 */
async function cylinderPartAndInstance(page) {
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(() => window.__waffle?.getState()?.sketchMode?.active === true, { timeout: 5000 });
	await page.evaluate(() => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: 0, y: 0, construction: true });
		w.addSketchEntity({ type: 'Circle', id: 2, center_id: 1, radius: 10, construction: false });
	});
	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForFunction(() => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= 1, { timeout: 10000 });
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.evaluate(() => window.__waffle.applyExtrude(20, 0, false));
	await page.waitForFunction(() => (window.__waffle?.getMeshes() ?? []).some((m) => m.triangleCount > 0), { timeout: 20000 });

	const partTab = await page.evaluate(() => window.__waffle.getDocumentState().activeTabId);
	await page.locator('[data-testid="tab-add-assembly"]').click();
	await page.waitForFunction(() => window.__waffle.getAssembly() !== null, { timeout: 10000 });
	const instance = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'Pin', fixed: true }), partTab);
	await page.waitForFunction(() => window.__waffle.getMeshes().length === 1, { timeout: 30000 });
	return { partTab, instance };
}

test.describe('Assemblies', () => {
	test('instances of a part render with their placements; a fastened mate stacks them', async ({ waffle }) => {
		const page = waffle.page;
		const partTab = await cubePartAndAssembly(page);
		// Switching to the assembly tab shows nothing until instances exist.
		expect(await page.evaluate(() => window.__waffle.getMeshes().length)).toBe(0);

		const a = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'A', fixed: true }), partTab);
		const b = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'B', transform: { translation_m: [0.03, 0, 0], rotation_quat: [0, 0, 0, 1] } }), partTab);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });
		const meshes = await page.evaluate(() => window.__waffle.getMeshes());
		expect(meshes.map((m) => m.instanceId).sort()).toEqual([a, b].sort());
		const mb = meshes.find((m) => m.instanceId === b);
		expect(mb.transform.translation_m).toEqual([0.03, 0, 0]);
		expect(mb.bodyId.startsWith(b + '/')).toBe(true);
		// The edge and vertex overlays follow the placement the faces follow:
		// B's edges are drawn at x = 0.03, and the vertex cloud reaches B's far
		// face (a 10 mm cube placed at 0.03 ends at 0.04). They used to render
		// every instance's edges and vertices at the origin.
		await page.waitForFunction(
			(n) => window.__waffle.getRenderedOverlayPlacements().edges.length === n,
			2,
			{ timeout: 10000 }
		);
		const overlays = await page.evaluate(() => window.__waffle.getRenderedOverlayPlacements());
		const edgesB = overlays.edges.find((e) => e.bodyId === mb.bodyId);
		expect(edgesB).toBeTruthy();
		expect(edgesB.position.map((v) => Math.round(v * 1e6) / 1e6)).toEqual([0.03, 0, 0]);
		expect(overlays.vertexBounds).toBeTruthy();
		expect(near(overlays.vertexBounds.max[0], 0.04)).toBe(true);
		expect(near(overlays.vertexBounds.min[0], 0)).toBe(true);
		const bodies = await page.evaluate(() => window.__waffle.getBodies?.() ?? []);
		void bodies;
		let status = await page.evaluate(() => window.__waffle.getAssemblyStatus());
		expect(status.errors).toEqual([]);
		// B is not grounded through mates yet → its own transform, with a warning.
		expect(status.warnings.some((w) => w.includes('`B`'))).toBe(true);
		expect(status.placements[b].translation_m).toEqual([0.03, 0, 0]);

		// Connectors: A's top face (explicit frame), B's bottom face; fasten (flip)
		// ⇒ B stacked on A: B's origin at z = 10 mm.
		const ca = await page.evaluate((id) => window.__waffle.addConnector({ instanceId: id, name: 'A top', frame: { origin: [0.005, 0.005, 0.01], z_axis: [0, 0, 1], x_axis: [0, 0, 0] } }), a);
		const cb = await page.evaluate((id) => window.__waffle.addConnector({ instanceId: id, name: 'B bottom', frame: { origin: [0.005, 0.005, 0], z_axis: [0, 0, -1], x_axis: [0, 0, 0] } }), b);
		await page.evaluate(([x, y]) => window.__waffle.addMate({ a: x, b: y, flip: true, name: 'stack' }), [ca, cb]);
		await page.waitForFunction((id) => {
			const p = window.__waffle.getAssemblyStatus()?.placements?.[id];
			return p && Math.abs(p.translation_m[2] - 0.01) < 1e-6 && Math.abs(p.translation_m[0]) < 1e-6;
		}, b, { timeout: 15000 });
		status = await page.evaluate(() => window.__waffle.getAssemblyStatus());
		expect(status.errors).toEqual([]);
		expect(status.warnings).toEqual([]);
		const placed = (await page.evaluate(() => window.__waffle.getMeshes())).find((m) => m.instanceId === b);
		expect(near(placed.transform.translation_m[2], 0.01)).toBe(true);
		expect(near(Math.abs(placed.transform.rotation_quat[3]), 1)).toBe(true);
		// The tab carries the derived placements.
		const asm = await page.evaluate(() => window.__waffle.getAssembly());
		expect(near(asm.placements[b].translation_m[2], 0.01)).toBe(true);
		expect(asm.instances).toHaveLength(2);
		expect(asm.mates[0].kind).toEqual({ type: 'Fastened', flip: true });

		// A connector on a REAL face: derived from the part's geometry.
		const faceRef = (await page.evaluate(() => window.__waffle.getMeshes()))[0].faceRanges[0].geom_ref;
		expect(faceRef?.kind?.type).toBe('Face');
		const cf = await page.evaluate(([id, ref]) => window.__waffle.addConnector({ instanceId: id, geomRef: ref, name: 'A face' }), [a, faceRef]);
		await page.waitForFunction((n) => (window.__waffle.getAssembly()?.connectors?.length ?? 0) === n, 3, { timeout: 10000 });
		status = await page.evaluate(() => window.__waffle.getAssemblyStatus());
		expect(status.errors).toEqual([]);
		expect(cf).toBeTruthy();

		// An over-constrained second mate is loud, and removing it clears the error.
		const m2 = await page.evaluate(([x, y]) => window.__waffle.addMate({ a: x, b: y, flip: true, rotationDeg: 45, name: 'contradiction' }), [ca, cb]);
		await page.waitForFunction(() => (window.__waffle.getAssemblyStatus()?.errors ?? []).some((e) => /over-constrained/.test(e)), { timeout: 10000 });
		await page.evaluate((id) => window.__waffle.removeMate(id), m2);
		await page.waitForFunction(() => (window.__waffle.getAssemblyStatus()?.errors ?? []).length === 0, { timeout: 10000 });
	});

	test('the assembly saves with the document and reopens on its tab from storage', async ({ waffle }) => {
		const page = waffle.page;
		const partTab = await cubePartAndAssembly(page);
		const a = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'A' }), partTab);
		const b = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'B', transform: { translation_m: [0, 0.02, 0], rotation_quat: [0, 0, 0, 1] } }), partTab);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });

		const json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		const asmTab = json.tabs.find((t) => t.kind.type === 'Assembly');
		expect(asmTab).toBeTruthy();
		expect(json.active_tab).toBe(asmTab.id);
		expect(asmTab.kind.assembly.instances.map((i) => i.name)).toEqual(['A', 'B']);
		expect(asmTab.kind.assembly.instances[0].source).toEqual({ tab_id: partTab });
		expect(asmTab.kind.assembly.placements[b].translation_m).toEqual([0, 0.02, 0]);
		// The Part tab kept its tree (the import feature) — the assembly did not
		// replace it with the empty live tree.
		const part = json.tabs.find((t) => t.id === partTab);
		expect(part.kind.features.features.map((f) => f.operation.type)).toEqual(['ImportedBody']);

		// Save to storage, reopen through /doc: the assembly tab is active and
		// its two instances render again.
		await page.keyboard.press('Control+s');
		await page.waitForFunction(() => (window.__waffle.getToasts() || []).some((t) => t.message === 'Saved'), { timeout: 10000 });
		const docId = await page.evaluate(() => window.__waffle.getDocumentState().activeDocId);
		const stored = await getDocumentFromDB(page, docId);
		expect(JSON.parse(stored.json).tabs.some((t) => t.kind.type === 'Assembly')).toBe(true);

		await page.goto(`/doc/${docId}`);
		await page.waitForURL('/', { timeout: 15000 });
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });
		await page.waitForFunction((id) => window.__waffle.getDocumentState().activeDocId === id, docId, { timeout: 15000 });
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });
		await expect(page.locator('[data-testid="assembly-panel"]')).toBeVisible();
		await expect(page.locator('[data-testid="asm-instance-1"]')).toContainText('cube.step'.replace('cube.step', 'Part'));
		void a;
	});

	test('the panel adds instances, moves them, and fastens connectors', async ({ waffle }) => {
		const page = waffle.page;
		await cubePartAndAssembly(page);
		await page.locator('[data-testid="asm-add-instance"]').click();
		await page.locator('[data-testid="asm-add-instance"]').click();
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });
		await expect(page.locator('[data-testid="asm-instance-1"]')).toBeVisible();

		// Move the second instance 25 mm in x through the panel.
		await page.locator('[data-testid="asm-instance-tx-1"]').fill('25');
		await page.locator('[data-testid="asm-instance-tx-1"]').press('Enter');
		await page.waitForFunction(() => {
			const asm = window.__waffle.getAssembly();
			const id = asm.instances[1].id;
			return Math.abs((asm.placements?.[id]?.translation_m?.[0] ?? 0) - 0.025) < 1e-9;
		}, { timeout: 15000 });

		// Rotate the second instance 90° about y through the panel: the
		// placement's quaternion is the XYZ-Euler conversion the viewport uses.
		await page.locator('[data-testid="asm-instance-ry-1"]').fill('90');
		await page.locator('[data-testid="asm-instance-ry-1"]').press('Enter');
		await page.waitForFunction(() => {
			const asm = window.__waffle.getAssembly();
			const q = asm.placements?.[asm.instances[1].id]?.rotation_quat ?? [0, 0, 0, 1];
			return Math.abs(q[1] - Math.SQRT1_2) < 1e-6 && Math.abs(q[3] - Math.SQRT1_2) < 1e-6;
		}, { timeout: 15000 });
		await expect(page.locator('[data-testid="asm-instance-ry-1"]')).toHaveValue('90');
		await page.locator('[data-testid="asm-instance-ry-1"]').fill('0');
		await page.locator('[data-testid="asm-instance-ry-1"]').press('Enter');
		await page.waitForFunction(() => {
			const asm = window.__waffle.getAssembly();
			const q = asm.placements?.[asm.instances[1].id]?.rotation_quat ?? [1, 1, 1, 0];
			return Math.abs(q[3]) > 0.999999;
		}, { timeout: 15000 });

		// Origin connectors on both, then fasten (no flip): B's origin frame
		// coincides with A's ⇒ B moves back onto A.
		await page.locator('[data-testid="asm-instance-origin-connector-0"]').click();
		await page.locator('[data-testid="asm-instance-origin-connector-1"]').click();
		await expect(page.locator('[data-testid="asm-connector-1"]')).toBeVisible();
		const ids = await page.evaluate(() => window.__waffle.getAssembly().connectors.map((c) => c.id));
		await page.locator('[data-testid="asm-mate-a"]').selectOption(ids[0]);
		await page.locator('[data-testid="asm-mate-b"]').selectOption(ids[1]);
		await page.locator('[data-testid="asm-mate-new-flip"]').uncheck();
		await page.locator('[data-testid="asm-add-mate"]').click();
		await expect(page.locator('[data-testid="asm-mate-0"]')).toContainText('Fastened');
		await page.waitForFunction(() => {
			const asm = window.__waffle.getAssembly();
			const id = asm.instances[1].id;
			const t = asm.placements?.[id]?.translation_m ?? [1, 1, 1];
			return Math.abs(t[0]) < 1e-9 && Math.abs(t[1]) < 1e-9 && Math.abs(t[2]) < 1e-9;
		}, { timeout: 15000 });
		expect(await page.evaluate(() => window.__waffle.getAssemblyStatus().errors)).toEqual([]);

		// Suppressing the mate returns B to its own placement (25 mm).
		await page.locator('[data-testid="asm-mate-suppressed-0"]').check();
		await page.waitForFunction(() => {
			const asm = window.__waffle.getAssembly();
			const id = asm.instances[1].id;
			return Math.abs((asm.placements?.[id]?.translation_m?.[0] ?? 0) - 0.025) < 1e-9;
		}, { timeout: 15000 });
	});
});

test.describe('Assemblies: numeric mates', () => {
	test('a revolute hinge aligns the axis, keeps the opening angle free, and a slider keeps its travel', async ({ waffle }) => {
		const page = waffle.page;
		const partTab = await cubePartAndAssembly(page);
		const a = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'A', fixed: true }), partTab);
		// B starts 30° open about y and displaced off the hinge.
		const s = Math.sin(Math.PI / 12), c = Math.cos(Math.PI / 12);
		const b = await page.evaluate(([t, s, c]) => window.__waffle.addInstance({ tabId: t, name: 'B', transform: { translation_m: [0.012, 0.003, 0.002], rotation_quat: [0, s, 0, c] } }), [partTab, s, c]);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });
		const ca = await page.evaluate((id) => window.__waffle.addConnector({ instanceId: id, name: 'A hinge', frame: { origin: [0.01, 0, 0.01], z_axis: [0, 1, 0], x_axis: [0, 0, 0] } }), a);
		const cb = await page.evaluate((id) => window.__waffle.addConnector({ instanceId: id, name: 'B hinge', frame: { origin: [0, 0, 0.01], z_axis: [0, 1, 0], x_axis: [0, 0, 0] } }), b);
		await page.evaluate(([x, y]) => window.__waffle.addMate({ a: x, b: y, kind: 'Revolute', flip: false, name: 'hinge' }), [ca, cb]);
		await page.waitForFunction(() => (window.__waffle.getAssemblyStatus()?.warnings ?? []).length === 0 && (window.__waffle.getAssembly()?.mates ?? []).length === 1, { timeout: 15000 });
		const status = await page.evaluate(() => window.__waffle.getAssemblyStatus());
		expect(status.errors).toEqual([]);
		const tb = status.placements[b];
		// B's hinge point (0,0,10mm in B) lands on A's hinge point (10,0,10mm).
		const q = tb.rotation_quat;
		const rot = (v) => {
			const [x, y, z, w] = q; const [vx, vy, vz] = v;
			const ix = w * vx + y * vz - z * vy, iy = w * vy + z * vx - x * vz, iz = w * vz + x * vy - y * vx, iw = -x * vx - y * vy - z * vz;
			return [ix * w + iw * -x + iy * -z - iz * -y, iy * w + iw * -y + iz * -x - ix * -z, iz * w + iw * -z + ix * -y - iy * -x];
		};
		const p = rot([0, 0, 0.01]).map((v, i) => v + tb.translation_m[i]);
		expect(near(p[0], 0.01)).toBe(true);
		expect(near(p[1], 0)).toBe(true);
		expect(near(p[2], 0.01)).toBe(true);
		// The opening angle stayed at 30° (free), the axis is y.
		const angle = 2 * Math.atan2(Math.hypot(q[0], q[1], q[2]), Math.abs(q[3])) * 180 / Math.PI;
		expect(Math.abs(angle - 30) < 0.05).toBe(true);
		expect(Math.abs(q[1] / Math.hypot(q[0], q[1], q[2])) > 0.9999).toBe(true);
		await expect(page.locator('[data-testid="asm-mate-0"]')).toContainText('Revolute');
		await expect(page.locator('[data-testid="asm-mate-kind-0"]')).toHaveValue('Revolute');

		// Change it to a Slider through the panel: the frames align fully and
		// B keeps its travel along the axis.
		await page.locator('[data-testid="asm-mate-kind-0"]').selectOption('Slider');
		await page.waitForFunction(() => window.__waffle.getAssembly().mates[0].kind.type === 'Slider', { timeout: 10000 });
		await page.waitForFunction(() => (window.__waffle.getAssemblyStatus()?.errors ?? []).length === 0, { timeout: 15000 });
		const slid = (await page.evaluate(() => window.__waffle.getAssemblyStatus())).placements[b];
		expect(Math.abs(slid.rotation_quat[3]) > 0.99999).toBe(true);
	});
});

test.describe('Assemblies: mate connector frames', () => {
	test('a connector on a cylindrical face derives the axis, and the panel says what it came from', async ({ waffle }) => {
		const page = waffle.page;
		const { instance } = await cylinderPartAndInstance(page);

		// Which face is the barrel? Ask the engine the way the app does before
		// creating anything — the probe judges a pick without minting one.
		const faces = await page.evaluate(() => window.__waffle.getMeshes()[0].faceRanges.map((r) => r.geom_ref));
		expect(faces.length).toBeGreaterThan(1);
		const probes = await page.evaluate(
			async ([id, refs]) => {
				const out = [];
				for (const ref of refs) out.push(await window.__waffle.probeConnectorRef([id], ref));
				return out;
			},
			[instance, faces]
		);
		const barrel = probes.findIndex((p) => p.ok && p.kind === 'cylindrical face');
		expect(barrel, 'the cylinder has a face the engine derives an axis from').toBeGreaterThanOrEqual(0);
		expect(probes.filter((p) => p.ok && p.kind === 'planar face').length, 'the two caps').toBe(2);

		const cid = await page.evaluate(
			([id, ref]) => window.__waffle.addConnector({ instanceId: id, geomRef: ref, name: 'barrel' }),
			[instance, faces[barrel]]
		);
		expect(cid).toBeTruthy();
		await page.waitForFunction(() => (window.__waffle.getAssemblyConnectorFrames() ?? []).length === 1, { timeout: 15000 });

		const status = await page.evaluate(() => window.__waffle.getAssemblyStatus());
		expect(status.errors).toEqual([]);
		const [frame] = await page.evaluate(() => window.__waffle.getAssemblyConnectorFrames());
		expect(frame.kind).toBe('cylindrical face');
		// On the axis, at the middle of the barrel's axial extent: half of 20.
		expect(near(frame.origin[0], 0, 1e-9)).toBe(true);
		expect(near(frame.origin[1], 0, 1e-9)).toBe(true);
		expect(near(frame.origin[2], 10, 1e-9)).toBe(true);
		expect(near(Math.abs(frame.z_axis[2]), 1, 1e-9)).toBe(true);

		await expect(page.locator('[data-testid="asm-connector-kind-0"]')).toContainText('cylindrical face');

		// Editing after creation (specs/assembly_connector_adjustments.md).
		// The extrude spans z = 0 … 20 (raw units): the anchor moves the frame
		// to the end its z points toward, the offset moves it along the
		// connector's OWN z (5000 mm = 5 units), and the flip reverses z with
		// the anchor following it to the other end.
		const frameAt = (pred, arg = null) =>
			page.waitForFunction(pred, arg, { timeout: 15000 }).then(() =>
				page.evaluate(() => window.__waffle.getAssemblyConnectorFrames()[0])
			);
		await page.evaluate((id) => window.__waffle.updateConnector(id, { anchor: 'positive_end' }), cid);
		const atEnd = await frameAt(() => Math.abs(window.__waffle.getAssemblyConnectorFrames()[0]?.origin[2] - 10) > 1);
		const sign = atEnd.z_axis[2] > 0 ? 1 : -1;
		const endZ = sign > 0 ? 20 : 0;
		expect(near(atEnd.origin[2], endZ, 1e-9), `at the end z points toward: ${JSON.stringify(atEnd)}`).toBe(true);
		await expect(page.locator('[data-testid="asm-connector-anchor-0"]')).toHaveValue('positive_end');

		await page.evaluate((id) => window.__waffle.updateConnector(id, { offsetMm: [0, 0, 5000] }), cid);
		const shifted = await frameAt(
			(z) => Math.abs(window.__waffle.getAssemblyConnectorFrames()[0]?.origin[2] - z) > 1,
			endZ
		);
		expect(near(shifted.origin[2], endZ + 5 * sign, 1e-9), `5 along z: ${JSON.stringify(shifted)}`).toBe(true);
		await expect(page.locator('[data-testid="asm-connector-oz-0"]')).toHaveValue('5000');

		await page.locator('[data-testid="asm-connector-flip-0"]').check();
		const flipped = await frameAt((s) => window.__waffle.getAssemblyConnectorFrames()[0]?.z_axis[2] * s < 0, sign);
		expect(near(flipped.z_axis[2], -sign, 1e-9)).toBe(true);
		// "+z end" is now the OTHER end, and the 5 along z now goes the other way.
		expect(near(flipped.origin[2], 20 - endZ - 5 * sign, 1e-9), `flipped: ${JSON.stringify(flipped)}`).toBe(true);
		expect(await page.evaluate(() => window.__waffle.getAssembly().connectors[0].flip_z)).toBe(true);

		// Renamed from the panel; the mate chooser lists the new name.
		await page.locator('[data-testid="asm-connector-name-0"]').fill('pin axis');
		await page.locator('[data-testid="asm-connector-name-0"]').press('Enter');
		await page.waitForFunction(() => window.__waffle.getAssembly().connectors[0].name === 'pin axis', { timeout: 10000 });
	});

	test('a pick the engine cannot derive a frame from is refused, and no connector is minted', async ({ waffle }) => {
		const page = waffle.page;
		const { instance } = await cylinderPartAndInstance(page);

		// A reference into a feature this part does not have: nothing resolves.
		const bogus = await page.evaluate(() => {
			const ref = JSON.parse(JSON.stringify(window.__waffle.getMeshes()[0].faceRanges[0].geom_ref));
			ref.anchor = { type: 'FeatureOutput', feature_id: '00000000-0000-4000-8000-000000000000', output_key: { type: 'Main' } };
			return ref;
		});
		const probe = await page.evaluate(([id, ref]) => window.__waffle.probeConnectorRef([id], ref), [instance, bogus]);
		expect(probe.ok).toBe(false);
		expect(probe.reason).toBeTruthy();

		const cid = await page.evaluate(
			([id, ref]) => window.__waffle.addConnector({ instanceId: id, geomRef: ref, name: 'nope' }),
			[instance, bogus]
		);
		expect(cid, 'the refused pick mints nothing').toBeNull();
		expect(await page.evaluate(() => window.__waffle.getAssembly().connectors?.length ?? 0)).toBe(0);
		await expect(page.locator('[data-testid="asm-connector-refusal"]')).toBeVisible();
	});
});

test.describe('Assemblies: sub-assemblies and linked parts', () => {
	test('an assembly can be instanced inside another; its members render with composed placements', async ({ waffle }) => {
		const page = waffle.page;
		const partTab = await cubePartAndAssembly(page);
		const asm1 = await page.evaluate(() => window.__waffle.getDocumentState().activeTabId);
		// Assembly 1: two cubes stacked by a fastened mate.
		const a = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'A', fixed: true }), partTab);
		const b = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'B' }), partTab);
		const ca = await page.evaluate((id) => window.__waffle.addConnector({ instanceId: id, name: 'A top', frame: { origin: [0.005, 0.005, 0.01], z_axis: [0, 0, 1], x_axis: [0, 0, 0] } }), a);
		const cb = await page.evaluate((id) => window.__waffle.addConnector({ instanceId: id, name: 'B bottom', frame: { origin: [0.005, 0.005, 0], z_axis: [0, 0, -1], x_axis: [0, 0, 0] } }), b);
		await page.evaluate(([x, y]) => window.__waffle.addMate({ a: x, b: y, flip: true }), [ca, cb]);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });

		// Assembly 2: two instances of Assembly 1, the second 50 mm over.
		await page.locator('[data-testid="tab-add-assembly"]').click();
		await page.waitForFunction((id) => window.__waffle.getDocumentState().activeTabId !== id && window.__waffle.getAssembly() !== null, asm1, { timeout: 10000 });
		await expect(page.locator('[data-testid="asm-add-instance-part"] optgroup[label="Assemblies"]')).toHaveCount(1);
		const s1 = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'Stack 1', fixed: true }), asm1);
		const s2 = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'Stack 2', transform: { translation_m: [0.05, 0, 0], rotation_quat: [0, 0, 0, 1] } }), asm1);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 4, { timeout: 30000 });
		const meshes = await page.evaluate(() => window.__waffle.getMeshes());
		const paths = meshes.map((m) => m.instancePath);
		expect(paths).toEqual([[s1, a], [s1, b], [s2, a], [s2, b]]);
		const s2b = meshes.find((m) => m.instancePath[0] === s2 && m.instancePath[1] === b);
		expect(near(s2b.transform.translation_m[0], 0.05)).toBe(true);
		expect(near(s2b.transform.translation_m[2], 0.01)).toBe(true);
		expect(s2b.bodyId.startsWith(`${s2}/${b}/`)).toBe(true);
		await expect(page.locator('[data-testid="asm-instance-part-0"]')).toContainText('(assembly)');
		expect((await page.evaluate(() => window.__waffle.getAssemblyStatus())).errors).toEqual([]);

		// A connector on a MEMBER (Stack 1 › B's top) fastens a lone cube at z = 20 mm.
		const lone = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'Lone' }), partTab);
		const cm = await page.evaluate(([top, mem]) => window.__waffle.addConnector({ instancePath: [top, mem], name: 'stack1 B top', frame: { origin: [0.005, 0.005, 0.01], z_axis: [0, 0, 1], x_axis: [0, 0, 0] } }), [s1, b]);
		const cl = await page.evaluate((id) => window.__waffle.addConnector({ instanceId: id, name: 'lone bottom', frame: { origin: [0.005, 0.005, 0], z_axis: [0, 0, -1], x_axis: [0, 0, 0] } }), lone);
		await page.evaluate(([x, y]) => window.__waffle.addMate({ a: x, b: y, flip: true }), [cm, cl]);
		await page.waitForFunction((id) => Math.abs((window.__waffle.getAssemblyStatus()?.placements?.[id]?.translation_m?.[2] ?? 0) - 0.02) < 1e-6, lone, { timeout: 15000 });
		await expect(page.locator('[data-testid="asm-connector-0"]')).toContainText('member');
		expect((await page.evaluate(() => window.__waffle.getAssemblyStatus())).errors).toEqual([]);
	});
});
