/**
 * Part mate connectors (specs/part_mate_connectors.md): a named connector is
 * authored in a Part through the dialog on a clicked face, built and reported
 * by the engine with its frame, edited, refused loudly on a pick with no
 * frame — then offered by every instance of the part in an assembly and
 * mated straight from the panel.
 *
 * `createExtrudedBox` builds in raw engine units: a 60 × 60 × 60 box on the
 * XY plane, top face at z = 60 (see helpers/geometry.js).
 */
import { test, expect } from './helpers/waffle-test.js';
import { createExtrudedBox, getVisibleFaces, clickFace } from './helpers/geometry.js';

const near = (a, b, tol = 1e-6) => Math.abs(a - b) <= tol;

/** Click the box's top face (the extrude's positive end cap). */
async function selectTopFace(page) {
	const faces = await getVisibleFaces(page);
	const top = faces.find((f) => f.geomRef?.selector?.role?.type === 'EndCapPositive');
	expect(top, JSON.stringify(faces.map((f) => f.geomRef?.selector))).toBeTruthy();
	await clickFace(page, top);
	await expect
		.poll(() => page.evaluate(() => window.__waffle.getSelectedRefs()[0]?.selector?.role?.type ?? null))
		.toBe('EndCapPositive');
	return top.geomRef;
}

/** Open the dialog on the current selection, name the connector, apply. */
async function addConnectorFromDialog(page, name, { oz = null } = {}) {
	await page.evaluate(() => window.__waffle.showMateConnectorDialog());
	const dialog = page.locator('[data-testid="mate-connector-dialog"]');
	await expect(dialog).toBeVisible();
	await expect(page.locator('[data-testid="mc-reference"]')).toHaveText('selected face');
	await page.locator('[data-testid="mc-name"]').fill(name);
	if (oz !== null) await page.locator('[data-testid="mc-oz"]').fill(String(oz));
	await page.locator('[data-testid="mc-apply"]').click();
	await expect(dialog).toBeHidden();
}

test.describe('Part mate connectors', () => {
	test('a connector on a clicked face is a feature with an evaluated frame, and edits in place', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		const topRef = await selectTopFace(page);
		await addConnectorFromDialog(page, 'Lid', { oz: 5 });

		const feature = await page.evaluate(() => window.__waffle.getFeatureTree().features.at(-1));
		expect(feature.operation.type).toBe('MateConnector');
		expect(feature.name).toBe('Lid');
		expect(feature.operation.params.offset_m).toEqual([0, 0, 0.005]);
		expect(feature.operation.params.geom_ref.selector.role.type).toBe('EndCapPositive');

		await page.waitForFunction(() => window.__waffle.getPartConnectors().length === 1, null, { timeout: 10000 });
		let [c] = await page.evaluate(() => window.__waffle.getPartConnectors());
		expect(c.feature_id).toBe(feature.id);
		expect(c.name).toBe('Lid');
		expect(c.kind).toBe('planar face');
		expect(near(c.origin[2], 60.005), JSON.stringify(c)).toBe(true);
		expect(near(c.z_axis[2], 1, 1e-9), JSON.stringify(c)).toBe(true);
		await expect(page.getByText('Lid', { exact: true }).first()).toBeVisible();

		// Edit: the dialog opens on the feature; flipping z moves the 5 mm offset
		// to the other side (it is along the connector's FINAL z).
		await page.evaluate((id) => window.__waffle.showEditFeatureDialog(id), feature.id);
		await expect(page.locator('[data-testid="mate-connector-dialog"]')).toBeVisible();
		await expect(page.locator('[data-testid="mc-name"]')).toHaveValue('Lid');
		await page.locator('[data-testid="mc-flip"]').check();
		await page.locator('[data-testid="mc-apply"]').click();
		await expect(page.locator('[data-testid="mate-connector-dialog"]')).toBeHidden();
		await page.waitForFunction(() => window.__waffle.getPartConnectors()[0]?.z_axis?.[2] < -0.5, null, { timeout: 10000 });
		[c] = await page.evaluate(() => window.__waffle.getPartConnectors());
		expect(near(c.origin[2], 59.995), JSON.stringify(c)).toBe(true);
		expect(await page.evaluate(() => window.__waffle.getFeatureTree().features.length)).toBe(3);

		// A pick with no frame (a vertex) fails its feature loudly; no frame is
		// reported for it and the dialog stays open on it.
		const vertexRef = { ...topRef, kind: { type: 'Vertex' } };
		await page.evaluate(() => window.__waffle.showMateConnectorDialog());
		const failedId = await page.evaluate((ref) => window.__waffle.applyMateConnector({ name: 'Bad', geomRef: ref }), vertexRef);
		expect(failedId).toBeTruthy();
		const errors = await page.evaluate(() => [...window.__waffle.getFeatureErrors().entries()]);
		expect(errors.some(([id]) => id === failedId), JSON.stringify(errors)).toBe(true);
		expect(await page.evaluate(() => window.__waffle.getPartConnectors().map((p) => p.name))).toEqual(['Lid']);
		expect((await page.evaluate(() => window.__waffle.getMateConnectorDialogState()))?.editingFeatureId).toBe(failedId);
		await expect(page.locator('[data-testid="mc-error"]')).toBeVisible();
	});

	test('every instance offers the part connector, and the panel mates two of them', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		await selectTopFace(page);
		await addConnectorFromDialog(page, 'Lid');
		await page.waitForFunction(() => window.__waffle.getPartConnectors().length === 1, null, { timeout: 10000 });
		const partTab = await page.evaluate(() => window.__waffle.getDocumentState().activeTabId);

		await page.locator('[data-testid="tab-add-assembly"]').click();
		await page.waitForFunction(() => window.__waffle.getAssembly() !== null, null, { timeout: 10000 });
		const a = await page.evaluate((t) => window.__waffle.addInstance({ tabId: t, name: 'A', fixed: true }), partTab);
		const b = await page.evaluate(
			(t) => window.__waffle.addInstance({ tabId: t, name: 'B', transform: { translation_m: [200, 0, 0], rotation_quat: [0, 0, 0, 1] } }),
			partTab
		);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, null, { timeout: 30000 });

		const offered = await page.evaluate(() => window.__waffle.getAssemblyPartConnectors());
		expect(offered.map((p) => [p.instance_path[0], p.name])).toEqual([
			[a, 'Lid'],
			[b, 'Lid']
		]);
		const onB = offered.find((p) => p.instance_path[0] === b);
		expect(near(onB.origin[0], 200) && near(onB.origin[2], 60), JSON.stringify(onB)).toBe(true);
		await expect(page.locator('[data-testid="asm-part-connector-0"]')).toContainText('A › Lid');
		await expect(page.locator('[data-testid="asm-part-connector-1"]')).toContainText('B › Lid');

		// Mate the two lids face to face (Fastened, flip): B lands upside down on A.
		await page.locator('[data-testid="asm-mate-a"]').selectOption('pc:0');
		await page.locator('[data-testid="asm-mate-b"]').selectOption('pc:1');
		await page.locator('[data-testid="asm-add-mate"]').click();
		await page.waitForFunction(
			(id) => {
				const p = window.__waffle.getAssemblyStatus()?.placements?.[id];
				return p && Math.abs(p.translation_m[2] - 120) < 1e-6;
			},
			b,
			{ timeout: 20000 }
		);
		const status = await page.evaluate(() => window.__waffle.getAssemblyStatus());
		expect(status.errors).toEqual([]);
		const pb = status.placements[b];
		expect(near(pb.translation_m[0], 0) && near(pb.translation_m[1], 0), JSON.stringify(pb)).toBe(true);
		expect(near(Math.abs(pb.rotation_quat[3]), 0, 1e-9), JSON.stringify(pb)).toBe(true);

		const asm = await page.evaluate(() => window.__waffle.getAssembly());
		expect(asm.connectors.map((c) => [c.name, c.part_connector != null])).toEqual([
			['A › Lid', true],
			['B › Lid', true]
		]);
		expect(status.connectors.every((f) => f.kind === 'part connector · planar face'), JSON.stringify(status.connectors)).toBe(true);
		// Both part connectors are in use now, so the panel offers none.
		await expect(page.locator('[data-testid="asm-part-connector-0"]')).toHaveCount(0);
	});
});
