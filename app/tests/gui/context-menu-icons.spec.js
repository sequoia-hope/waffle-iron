/**
 * Context-menu icons (ui-chrome M13 step 6, specs/ui_toolbar_icons.md §1e).
 *
 * Every row of every right-click menu leads with exactly one icon and keeps
 * its words. Each check walks the rows that are actually rendered rather
 * than a list, so a new row without an icon fails here.
 */
import { test, expect } from './helpers/waffle-test.js';
import { getCanvasBounds, drawLine } from './helpers/canvas.js';
import { clickSketch, clickSelect, pressKey } from './helpers/toolbar.js';
import { createExtrudedBox } from './helpers/geometry.js';
import { waitForEntityCount } from './helpers/state.js';
import { setSketchSelection } from './helpers/constraint.js';

/** Each rendered row under `menuSelector`: its icon count, words and icon markup. */
async function rows(page, menuSelector, rowSelector) {
	return page.evaluate(
		([menuSel, rowSel]) => {
			const menu = document.querySelector(menuSel);
			if (!menu) return null;
			return [...menu.querySelectorAll(rowSel)].map((b) => ({
				id: b.dataset.testid ?? '',
				icons: b.querySelectorAll('svg.wi-icon').length,
				leads: b.firstElementChild?.matches('svg.wi-icon') ?? false,
				text: (b.textContent ?? '').trim(),
				markup: b.querySelector('svg.wi-icon')?.innerHTML ?? ''
			}));
		},
		[menuSelector, rowSelector]
	);
}

/** Close the tree's menu with a click on empty canvas (any window click closes it). */
async function dismissTreeMenu(page) {
	const b = await getCanvasBounds(page);
	await page.mouse.click(b.x + 40, b.y + b.height - 40);
	await expect(page.locator('.context-menu')).toHaveCount(0, { timeout: 3000 });
}

function expectEveryRowHasOneLeadingIcon(list, what) {
	expect(list, `${what} is open`).not.toBeNull();
	expect(list.length, `${what} has rows`).toBeGreaterThan(0);
	for (const r of list) {
		expect(r.icons, `${what} row "${r.text}" has one icon`).toBe(1);
		expect(r.leads, `${what} row "${r.text}" leads with its icon`).toBe(true);
		expect(r.markup.length, `${what} row "${r.text}" icon is not empty`).toBeGreaterThan(0);
		expect(r.text.length, `${what} row ${r.id} keeps its words`).toBeGreaterThan(0);
	}
}

test.describe('context menus lead every row with an icon', () => {
	test('viewport menu; the four views each have their own picture', async ({ waffle }) => {
		const page = waffle.page;
		const bounds = await getCanvasBounds(page);
		await page.mouse.click(bounds.centerX, bounds.centerY, { button: 'right' });
		await expect(page.getByTestId('ctx-menu')).toBeVisible({ timeout: 3000 });

		const list = await rows(page, '[data-testid="ctx-menu"]', '.ctx-item');
		expectEveryRowHasOneLeadingIcon(list, 'viewport menu');
		const views = ['ctx-view-front', 'ctx-view-top', 'ctx-view-right', 'ctx-view-iso'].map(
			(id) => list.find((r) => r.id === id)
		);
		for (const v of views) expect(v, 'every view row is present').toBeTruthy();
		expect(new Set(views.map((v) => v.markup)).size, 'four different view pictures').toBe(4);
	});

	test('feature, origin and body menus in the tree', async ({ waffle }) => {
		const page = waffle.page;
		await createExtrudedBox(page);
		const items = page.locator('.tree-item:not(.origin-item)');
		await expect(items).toHaveCount(2);

		// The sketch: edit sketch, rename, suppress, delete, hide sketches.
		await items.nth(0).click({ button: 'right' });
		await expect(page.getByTestId('ft-ctx-edit-sketch')).toBeVisible({ timeout: 3000 });
		expectEveryRowHasOneLeadingIcon(await rows(page, '.context-menu', '.ctx-item'), 'sketch menu');
		await dismissTreeMenu(page);

		// The extrude: edit feature instead of edit sketch. Suppress and
		// unsuppress show different pictures.
		await items.nth(1).click({ button: 'right' });
		await expect(page.getByTestId('ft-ctx-edit-feature')).toBeVisible({ timeout: 3000 });
		const extrudeRows = await rows(page, '.context-menu', '.ctx-item');
		expectEveryRowHasOneLeadingIcon(extrudeRows, 'extrude menu');
		const suppressPicture = extrudeRows.find((r) => r.id === 'ft-ctx-suppress').markup;
		await page.getByTestId('ft-ctx-suppress').click();
		await expect(items.nth(1)).toHaveClass(/suppressed/, { timeout: 10000 });
		await items.nth(1).click({ button: 'right' });
		await expect(page.getByTestId('ft-ctx-suppress')).toHaveText('Unsuppress', { timeout: 3000 });
		const unsuppressRow = (await rows(page, '.context-menu', '.ctx-item')).find((r) => r.id === 'ft-ctx-suppress');
		expect(unsuppressRow.icons).toBe(1);
		expect(unsuppressRow.markup, 'Unsuppress is a different picture from Suppress').not.toBe(suppressPicture);
		await page.getByTestId('ft-ctx-suppress').click();
		await expect(items.nth(1)).not.toHaveClass(/suppressed/, { timeout: 10000 });

		// An origin plane: hide or show all planes.
		await page.getByTestId('origin-plane-front').click({ button: 'right' });
		await expect(page.locator('.context-menu .ctx-item')).toHaveCount(1, { timeout: 3000 });
		expectEveryRowHasOneLeadingIcon(await rows(page, '.context-menu', '.ctx-item'), 'origin menu');
		await dismissTreeMenu(page);

		// A body: export STL.
		await page.getByTestId('body-item-0').click({ button: 'right' });
		await expect(page.getByTestId('body-ctx-export-stl')).toBeVisible({ timeout: 3000 });
		expectEveryRowHasOneLeadingIcon(await rows(page, '.context-menu', '.ctx-item'), 'body menu');
	});

	test('sketch constraint menu: every offered constraint shows its picture', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await drawLine(page, -10, -110, 25, 110);
		await waitForEntityCount(page, 3, 5000);
		await pressKey(page, 'Escape');
		await pressKey(page, 'l');
		await drawLine(page, -140, -40, 70, 35);
		await waitForEntityCount(page, 6, 5000);
		await clickSelect(page);
		const lines = (await page.evaluate(() => window.__waffle.getEntities())).filter((e) => e.type === 'Line');
		expect(lines.length).toBe(2);

		// Two lines offer the most rows (parallel, perpendicular, equal, angle…).
		await setSketchSelection(page, [lines[0].id, lines[1].id]);
		const bounds = await getCanvasBounds(page);
		await page.mouse.click(bounds.centerX + 200, bounds.centerY - 150, { button: 'right' });
		await expect(page.locator('.constraint-menu')).toBeVisible({ timeout: 3000 });
		const list = await rows(page, '.constraint-menu', '[role="menuitem"]');
		expectEveryRowHasOneLeadingIcon(list, 'constraint menu');
		expect(list.length, 'two lines offer several constraints').toBeGreaterThan(2);
		// The icon is decoration: the row is still found by its words.
		await expect(
			page.locator('.constraint-menu').getByRole('menuitem', { name: 'Parallel', exact: true })
		).toBeVisible();
	});
});
