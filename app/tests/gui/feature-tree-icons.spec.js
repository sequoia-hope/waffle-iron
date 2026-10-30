/**
 * Feature-tree icons (ui-chrome M13 step 7, specs/ui_toolbar_icons.md).
 *
 * A feature row shows the same picture as the toolbar button that made it,
 * and every tree icon (features, origin planes and axes, bodies) is a
 * registry icon, not a Unicode glyph.
 */
import { test, expect } from './helpers/waffle-test.js';
import { createExtrudedBox } from './helpers/geometry.js';

test('every tree row leads with a registry icon; features match their tool', async ({ waffle }) => {
	const page = waffle.page;
	await createExtrudedBox(page);
	await expect(page.locator('.tree-item:not(.origin-item)')).toHaveCount(2);
	await expect(page.getByTestId('body-item-0')).toBeVisible();

	const icons = await page.evaluate(() =>
		[...document.querySelectorAll('.feature-tree .tree-icon')].map((el) => ({
			row: el.closest('[data-testid]')?.dataset.testid ?? el.parentElement?.className ?? '',
			name: el.dataset.icon ?? null,
			svgs: el.querySelectorAll('svg.wi-icon').length,
			text: (el.textContent ?? '').trim()
		}))
	);
	// Two features, three planes, three axes, one body at least.
	expect(icons.length).toBeGreaterThanOrEqual(9);
	for (const i of icons) {
		expect(i.svgs, `tree icon in ${i.row} is one registry icon`).toBe(1);
		expect(i.text, `tree icon in ${i.row} has no leftover glyph`).toBe('');
	}

	// The sketch and the extrude show the toolbar's Sketch and Extrude pictures.
	const featureIcons = icons.filter((i) => i.name).map((i) => i.name);
	expect(featureIcons).toEqual(['sketch', 'extrude']);
	const toolbarMarkup = await page.evaluate(() =>
		['sketch', 'extrude'].map(
			(id) => document.querySelector(`[data-testid="toolbar-btn-${id}"] svg.wi-icon`)?.innerHTML ?? null
		)
	);
	const treeMarkup = await page.evaluate(() =>
		[...document.querySelectorAll('.feature-tree .tree-icon[data-icon] svg.wi-icon')].map((s) => s.innerHTML)
	);
	// A toolbar button can sit in "More ▾" at this width; compare those that are inline.
	toolbarMarkup.forEach((m, k) => {
		if (m != null) expect(treeMarkup[k], `tree row ${k} matches its toolbar button`).toBe(m);
	});
});
