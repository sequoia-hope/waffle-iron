/**
 * P2 — the DOCUMENT parameter scope and per-instance overrides, end to end
 * through the panels (`specs/agent_mechanical_design.md` §6).
 *
 * `parameterized-designs.spec.js` covers the tab-scoped loop. What is new
 * here is what only a browser can check: that the two panel affordances
 * exist, reach the engine, and show the truth back.
 *
 *  1. The "Document variables" section drives a field in a Part tab, and a
 *     tab row of the same name SHADOWS it visibly rather than silently.
 *  2. A document variable survives a tab switch, because it is the
 *     document's and not the tab's.
 *  3. The assembly panel's per-instance "vars" row makes two instances of one
 *     Part tab build different depths, and blanking a field puts the instance
 *     back on the part's own value.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	getFeatureTree,
	collectCrashErrors,
	expectNoAnyCrash,
} from './helpers/state.js';

/** Add a row to one of the two variable sections through the panel UI. */
async function addVariable(page, scope, name, expression) {
	const prefix = scope === 'document' ? 'document-variable' : 'variable';
	if (scope === 'document') {
		// The section is collapsed by default; + expands it.
		await page.locator('[data-testid="document-variable-add"]').click();
	} else {
		await page.locator('[data-testid="variable-add"]').click();
	}
	const nameInput = page.locator('[data-testid="variable-name-input"]');
	await nameInput.waitFor({ state: 'visible', timeout: 3000 });
	await nameInput.fill(name);
	const exprInput = page.locator('[data-testid="variable-expr-input"]');
	await exprInput.fill(expression);
	await exprInput.press('Enter');
	await page.locator(`[data-testid="${prefix}-row-${name}"]`).waitFor({ timeout: 5000 });
}

/** A rectangle sketch extruded by `expression`. */
async function plate(page, expression) {
	await clickSketch(page);
	await clickRectangle(page);
	await drawRectangle(page, -80, -60, 80, 60);
	await waitForEntityCount(page, 8, 5000);
	await clickFinishSketch(page);
	await waitForFeatureCount(page, 1, 10000);
	await clickExtrude(page);
	await page.locator('[data-testid="extrude-depth"]').fill(expression);
	await page.locator('[data-testid="extrude-apply"]').click();
	await waitForFeatureCount(page, 2, 10000);
}

async function extrudeDepth(page) {
	const tree = await getFeatureTree(page);
	return tree.features.find((f) => f.operation?.type === 'Extrude')?.operation?.params?.depth;
}

test.describe('document variables', () => {
	test('a document variable drives a tab field, and a tab row of the same name shadows it', async ({
		waffle,
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await addVariable(page, 'document', 'stock', '12');
		expect(await page.evaluate(() => window.__waffle.getDocumentParameters())).toMatchObject([
			{ name: 'stock', expression: '12', value: 12 },
		]);
		// And it is NOT in the tab's table — that is the whole point.
		expect(await page.evaluate(() => window.__waffle.getParameters())).toEqual([]);

		await plate(page, 'stock * 2');
		expect(await extrudeDepth(page)).toBeCloseTo(0.024, 12);

		// A tab row of the same name shadows the document one, totally.
		await addVariable(page, 'tab', 'stock', '3');
		await page.waitForFunction(
			() => {
				const t = window.__waffle.getFeatureTree();
				const e = t.features.find((f) => f.operation?.type === 'Extrude');
				return e && Math.abs(e.operation.params.depth - 0.006) < 1e-12;
			},
			null,
			{ timeout: 10000 }
		);
		expect(await extrudeDepth(page)).toBeCloseTo(0.006, 12);

		// The panel SAYS so: the shadowed document row is marked, not hidden.
		// "Why is my document variable not driving this" has to be answerable
		// from the panel.
		const shadowed = page.locator('[data-testid="document-variable-row-stock"]');
		await expect(shadowed).toHaveClass(/variable-shadowed/);
		await expect(shadowed).toHaveAttribute('title', /SHADOWED/);
		// And it still reports its own value — it is listed to be recognised.
		await expect(page.locator('[data-testid="document-variable-value-stock"]')).toHaveText('12');

		expectNoAnyCrash(crashes);
	});

	test('a document variable survives a tab switch and drives the new tab too', async ({
		waffle,
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		await addVariable(page, 'document', 'stock', '8');
		await plate(page, 'stock');
		expect(await extrudeDepth(page)).toBeCloseTo(0.008, 12);

		// A second Part tab reads the same table — which a tab-scoped table
		// could not do.
		await page.locator('[data-testid="tab-add"]').click();
		await page.waitForFunction(() => window.__waffle.getDocumentTabs().length === 2, null, {
			timeout: 10000,
		});
		expect(await page.evaluate(() => window.__waffle.getDocumentParameters())).toMatchObject([
			{ name: 'stock', value: 8 },
		]);
		await plate(page, 'stock * 3');
		expect(await extrudeDepth(page)).toBeCloseTo(0.024, 12);

		expectNoAnyCrash(crashes);
	});

	test('a document variable cannot be renamed in place, and says what to do instead', async ({
		waffle,
	}) => {
		const page = waffle.page;
		await addVariable(page, 'document', 'stock', '12');

		await page.locator('[data-testid="document-variable-row-stock"]').click();
		const nameInput = page.locator('[data-testid="variable-name-input"]');
		await nameInput.waitFor({ state: 'visible', timeout: 3000 });
		await nameInput.fill('plate_t');
		await page.locator('[data-testid="variable-expr-input"]').press('Enter');

		// Refused, loudly, with the three-step workaround — because the
		// rewrite would have to reach every tab's expressions.
		await expect(page.locator('.toast').filter({ hasText: 'cannot be renamed' })).toBeVisible({
			timeout: 5000,
		});
		expect(await page.evaluate(() => window.__waffle.getDocumentParameters())).toMatchObject([
			{ name: 'stock' },
		]);
	});
});

test.describe('per-instance parameter overrides', () => {
	test('two instances of one part build different depths, and blanking restores the part value', async ({
		waffle,
	}) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// A parameterised plate in Part 1.
		await addVariable(page, 'tab', 'height', '10');
		await plate(page, 'height');
		expect(await extrudeDepth(page)).toBeCloseTo(0.010, 12);
		const partTab = await page.evaluate(() => window.__waffle.getDocumentState().activeTabId);

		// An assembly tab with two instances of it.
		await page.locator('[data-testid="tab-add-assembly"]').click();
		await page.waitForFunction(() => window.__waffle.getAssembly() !== null, { timeout: 10000 });
		for (let i = 0; i < 2; i++) {
			await page.evaluate((tabId) => window.__waffle.addInstance({ tabId }), partTab);
		}
		await page.waitForSelector('[data-testid="asm-instance-1"]', { timeout: 20000 });

		// The override row is there, and offers one field per PART parameter.
		await page.locator('[data-testid="asm-instance-overrides-toggle-1"]').click();
		const field = page.locator('[data-testid="asm-instance-override-height-1"]');
		await expect(field).toBeVisible();
		// Empty, with the part's own value as the placeholder.
		await expect(field).toHaveValue('');
		await expect(field).toHaveAttribute('placeholder', '10');

		// Override instance B only.
		await field.fill('40');
		await field.press('Enter');
		await field.blur();
		await page.waitForFunction(
			() => {
				const insts = window.__waffle.getAssembly()?.instances ?? [];
				return insts.length === 2 && insts[1].parameter_overrides?.height === 40;
			},
			null,
			{ timeout: 20000 }
		);

		// A is untouched, B is its own build.
		const overrides = await page.evaluate(() =>
			window.__waffle.getAssembly().instances.map((i) => i.parameter_overrides?.height ?? null)
		);
		expect(overrides).toEqual([null, 40]);

		// Blanking the field clears the override, so the instance is the
		// part's own build again and the file carries no key at all.
		await field.fill('');
		await field.press('Enter');
		await field.blur();
		await page.waitForFunction(
			() => window.__waffle.getAssembly()?.instances?.[1]?.parameter_overrides === undefined,
			null,
			{ timeout: 20000 }
		);

		expectNoAnyCrash(crashes);
	});
});
