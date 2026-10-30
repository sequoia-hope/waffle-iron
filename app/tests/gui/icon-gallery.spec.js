/**
 * The icon registry renders (ui-chrome M13, specs/ui_toolbar_icons.md).
 *
 * `Icon.svelte` injects each glyph with `{@html}` inside an `<svg>`. The
 * children must be created in the SVG namespace, or the browser keeps them as
 * unknown HTML elements that draw nothing, with no error anywhere. The
 * `/dev/icons` gallery renders every registered name, so this spec checks
 * each one: its markup parses into SVG-namespace elements and draws a
 * non-empty box.
 */
import { test, expect } from '@playwright/test';

const SVG_NS = 'http://www.w3.org/2000/svg';

test('every registered icon renders as SVG with a visible extent', async ({ page }) => {
	await page.goto('/dev/icons');
	await expect(page.getByTestId('icon-gallery')).toBeVisible({ timeout: 15000 });

	const report = await page.evaluate((ns) => {
		const cells = [...document.querySelectorAll('[data-testid^="icon-cell-"]')];
		return cells.map((cell) => {
			const svg = cell.querySelector('svg.wi-icon');
			const kids = svg ? [...svg.children] : [];
			const box = svg ? svg.getBBox() : { width: 0, height: 0 };
			return {
				name: cell.getAttribute('data-testid').slice('icon-cell-'.length),
				svgs: cell.querySelectorAll('svg.wi-icon').length,
				children: kids.length,
				foreign: kids.filter((k) => k.namespaceURI !== ns).map((k) => k.tagName),
				w: box.width,
				h: box.height
			};
		});
	}, SVG_NS);

	expect(report.length).toBeGreaterThan(90);
	const bad = report.filter(
		(r) => r.svgs !== 5 || r.children === 0 || r.foreign.length > 0 || r.w <= 0 || r.h <= 0
	);
	expect(bad, JSON.stringify(bad, null, 1)).toEqual([]);
});
