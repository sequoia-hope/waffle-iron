/**
 * Custom feature scripts in the page — A-M4 of
 * `specs/custom_features_and_modeling_roadmap.md` (§A8): the toolbar's
 * "Script" dialog whose fields are GENERATED from the script's `@param`
 * header, and the script editor (Check with the failing line, Save that
 * regenerates every node using the source).
 *
 * Covers: a new script from the editor becomes a source the dialog lists and
 * builds a body with the exact box extents; double-click edits the node with
 * its saved values and Apply replaces them; an expression argument that does
 * not evaluate disables Apply; the editor names the failing line of a broken
 * header and refuses nothing (work in progress saves); a Save that breaks the
 * node at runtime is loud on the node and in the editor, and the next Save
 * clears it; the built-in gear library script builds; the Script node shows
 * its arguments in the property editor; Escape closes.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	waitForFeatureCount,
	hasFeatureOfType,
	hasMeshWithGeometry,
	getFeatureTree,
	getMeshBoundingBox,
	collectCrashErrors,
	expectNoAnyCrash
} from './helpers/state.js';

/** A box script: 20 × 10 × 5 mm by default, on a plane parameter. */
const BOX_SCRIPT = `// @feature name="Box" version=1
// @param width: length = 0.02 min=0.001
// @param height: length = 0.01
// @param depth: length = 0.005
// @param plane: plane
// @output body: main
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.width, p.height);
    let r = sk.finish().regions();
    ctx.extrude(r[0], #{ depth: p.depth })
}
`;

async function openScriptDialog(page) {
	await page.locator('[data-testid="toolbar-btn-script"]').click();
	await page.locator('[data-testid="script-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
}

/** Add a script source through the editor (the way a user types one). */
async function addScriptViaEditor(page, text) {
	await openScriptDialog(page);
	await page.locator('[data-testid="script-source"]').selectOption('__new__');
	const editor = page.locator('[data-testid="script-editor"]');
	await expect(editor).toBeVisible();
	await page.locator('[data-testid="script-editor-text"]').fill(text);
	await page.locator('[data-testid="script-editor-save-close"]').click();
	await expect(editor).toBeHidden();
	// The dialog now points at the new source and shows its interface.
	await expect(page.locator('[data-testid="script-iface-name"]')).toContainText('v1');
	return page.evaluate(() => window.__waffle.getScriptDialogState()?.sourceId ?? null);
}

async function featureErrors(page) {
	return page.evaluate(() => Object.fromEntries(window.__waffle.getFeatureErrors()));
}

function extents(bbox) {
	return [bbox.max[0] - bbox.min[0], bbox.max[1] - bbox.min[1], bbox.max[2] - bbox.min[2]].sort((a, b) => a - b);
}

test.describe('script dialog and editor', () => {
	test('a new script from the editor becomes a source; the generated dialog builds the exact box', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await openScriptDialog(page);
		// No script sources yet: nothing to apply.
		await expect(page.locator('[data-testid="script-apply"]')).toBeDisabled();

		const sourceId = await addScriptViaEditor(page, BOX_SCRIPT);
		expect(sourceId).toBeTruthy();
		const sources = await page.evaluate(() => window.__waffle.getScriptSources());
		expect(sources.map((s) => s.name)).toEqual(['Box']);
		expect(sources[0].kind).toBe('Script');
		expect(sources[0].available).toBe(true);

		// The fields come from the header: three lengths (in the display unit,
		// defaulted) and a plane.
		for (const name of ['width', 'height', 'depth', 'plane']) {
			await expect(page.locator(`[data-testid="script-param-${name}"]`)).toBeVisible();
		}
		await expect(page.locator('[data-testid="script-input-width"]')).toHaveValue('20');
		await expect(page.locator('[data-testid="script-input-depth"]')).toHaveValue('5');
		await expect(page.locator('[data-testid="script-apply"]')).toBeEnabled();

		await page.locator('[data-testid="script-apply"]').click();
		await waitForFeatureCount(page, 1, 20000);
		await expect(page.locator('[data-testid="script-dialog"]')).toBeHidden();
		expect(await hasFeatureOfType(page, 'Script')).toBe(true);
		expect(await hasMeshWithGeometry(page)).toBe(true);
		const tree = await getFeatureTree(page);
		const node = tree.features[0];
		expect(node.name).toBe('Box');
		expect(node.operation.params.source_id).toBe(sourceId);
		expect(node.operation.params.args.width).toBeCloseTo(0.02, 9);
		expect(node.operation.params.args.plane.normal).toEqual([0, 0, 1]);
		expect(await featureErrors(page)).toEqual({});
		const bbox = await getMeshBoundingBox(page);
		const e = extents(bbox);
		expect(e[0]).toBeCloseTo(0.005, 6);
		expect(e[1]).toBeCloseTo(0.01, 6);
		expect(e[2]).toBeCloseTo(0.02, 6);

		// The property editor lists the node's arguments.
		await page.locator('[data-testid="feature-item-0"]').click();
		await expect(page.locator('[data-testid="property-editor"]')).toContainText('width');

		// Double-click edits with the saved values; Apply replaces them.
		await page.locator('[data-testid="feature-item-0"]').dblclick();
		await expect(page.locator('[data-testid="script-dialog"]')).toBeVisible();
		await expect(page.locator('[data-testid="script-input-width"]')).toHaveValue('20');
		await page.locator('[data-testid="script-input-width"]').fill('30');
		await page.locator('[data-testid="script-apply"]').click();
		await expect(page.locator('[data-testid="script-dialog"]')).toBeHidden();
		await page.waitForFunction(() => {
			const f = window.__waffle.getFeatureTree()?.features?.[0];
			return f && Math.abs(f.operation.params.args.width - 0.03) < 1e-12;
		}, null, { timeout: 15000 });
		expect(await featureErrors(page)).toEqual({});
		expect(extents(await getMeshBoundingBox(page))[2]).toBeCloseTo(0.03, 6);
		expect((await getFeatureTree(page)).features.length).toBe(1);

		expectNoAnyCrash(crashes);
	});

	test('an expression argument that does not evaluate disables Apply; a valid one is stored as arg_exprs', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await addScriptViaEditor(page, BOX_SCRIPT);

		await page.locator('[data-testid="script-input-depth"]').fill('nope +');
		await expect(page.locator('[data-testid="script-eval-depth"]')).toBeVisible();
		await expect(page.locator('[data-testid="script-apply"]')).toBeDisabled();

		// A design parameter makes the expression evaluate (mm-space).
		await page.evaluate(() => window.__waffle.setParameters([{ name: 'd', expression: '7' }]));
		await page.locator('[data-testid="script-input-depth"]').fill('d');
		await expect(page.locator('[data-testid="script-eval-depth"]')).toContainText('= 7 mm');
		await expect(page.locator('[data-testid="script-apply"]')).toBeEnabled();
		await page.locator('[data-testid="script-apply"]').click();
		await waitForFeatureCount(page, 1, 20000);
		const tree = await getFeatureTree(page);
		expect(tree.features[0].operation.params.arg_exprs).toEqual({ depth: 'd' });
		expect(tree.features[0].operation.params.args.depth).toBeUndefined();
		expect(await featureErrors(page)).toEqual({});
		expect(extents(await getMeshBoundingBox(page))[0]).toBeCloseTo(0.007, 6);
		expectNoAnyCrash(crashes);
	});

	test('the editor names the failing line, saves work in progress, and a breaking save is loud on the node', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		const sourceId = await addScriptViaEditor(page, BOX_SCRIPT);
		await page.locator('[data-testid="script-apply"]').click();
		await waitForFeatureCount(page, 1, 20000);

		// Open the editor from the dialog on the node.
		await page.locator('[data-testid="feature-item-0"]').dblclick();
		await page.locator('[data-testid="script-edit-source"]').click();
		const editor = page.locator('[data-testid="script-editor"]');
		await expect(editor).toBeVisible();
		await expect(page.locator('[data-testid="script-editor-status"]')).toContainText('Box v1');
		await expect(page.locator('[data-testid="script-editor-meta"]')).toContainText('1 feature');

		// A broken header: the check names line 2.
		const text = page.locator('[data-testid="script-editor-text"]');
		await text.fill(BOX_SCRIPT.replace('// @param width: length', '// @param width: nope'));
		await page.locator('[data-testid="script-editor-check"]').click();
		await expect(page.locator('[data-testid="script-editor-error"]')).toContainText('header: line 2:');

		// A save that breaks the node at runtime (regions[7]) is accepted —
		// the source is the user's — and the node's error is loud in the
		// tree and in the editor.
		await text.fill(BOX_SCRIPT.replace('r[0]', 'r[7]'));
		await expect(page.locator('[data-testid="script-editor-status"]')).toContainText('Box v1');
		await page.locator('[data-testid="script-editor-save"]').click();
		await expect(page.locator('[data-testid="script-editor-node-error"]')).toBeVisible({ timeout: 15000 });
		await expect(page.locator('[data-testid="feature-error-0"]')).toBeVisible();
		const errors = await featureErrors(page);
		expect(Object.values(errors)[0]).toContain('runtime');
		expect(await hasMeshWithGeometry(page)).toBe(false);

		// Fixing it clears the error and the body is back.
		await text.fill(BOX_SCRIPT);
		await page.locator('[data-testid="script-editor-save-close"]').click();
		await expect(editor).toBeHidden();
		await page.waitForFunction(() => window.__waffle.getFeatureErrors().size === 0, null, { timeout: 15000 });
		expect(await hasMeshWithGeometry(page)).toBe(true);
		const sources = await page.evaluate(() => window.__waffle.getScriptSources());
		expect(sources.map((s) => s.id)).toEqual([sourceId]);

		// The Sources panel offers the editor for a script source.
		await page.locator('[data-testid="script-cancel"]').click();
		await page.locator('[data-testid="source-edit-0"]').click();
		await expect(editor).toBeVisible();
		await page.keyboard.press('Escape');
		await expect(editor).toBeHidden();
		expectNoAnyCrash(crashes);
	});

	test('the built-in gear script is added from the dialog and builds', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await openScriptDialog(page);
		await page.locator('[data-testid="script-source"]').selectOption('lib:gear');
		await expect(page.locator('[data-testid="script-iface-name"]')).toContainText('Spur gear');
		await expect(page.locator('[data-testid="script-input-tooth_count"]')).toHaveValue('20');
		await expect(page.locator('[data-testid="script-input-internal"]')).not.toBeChecked();
		await page.locator('[data-testid="script-input-tooth_count"]').fill('12');
		await page.locator('[data-testid="script-apply"]').click();
		await waitForFeatureCount(page, 1, 60000);
		await page.waitForFunction(() => (window.__waffle.getMeshes() ?? []).some((m) => m.triangleCount > 0), null, { timeout: 60000 });
		const tree = await getFeatureTree(page);
		expect(tree.features[0].name).toBe('Spur gear');
		expect(tree.features[0].operation.params.args.tooth_count).toBe(12);
		expect(await featureErrors(page)).toEqual({});
		const sources = await page.evaluate(() => window.__waffle.getScriptSources());
		expect(sources.map((s) => s.name)).toEqual(['Spur gear']);
		expectNoAnyCrash(crashes);
	});

	test('Escape closes the dialog without a feature', async ({ waffle }) => {
		const page = waffle.page;
		await addScriptViaEditor(page, BOX_SCRIPT);
		await page.keyboard.press('Escape');
		await expect(page.locator('[data-testid="script-dialog"]')).toBeHidden();
		expect((await getFeatureTree(page)).features.length).toBe(0);
	});
});
