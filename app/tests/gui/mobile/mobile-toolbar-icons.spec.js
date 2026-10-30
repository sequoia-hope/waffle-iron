/**
 * Toolbar icons on mobile (ui-chrome M13, specs/ui_toolbar_icons.md): icons
 * only. The label stays in the DOM, visually hidden, so it is still the
 * button's accessible name and text.
 */
import { test, expect } from '../helpers/waffle-test.js';

test('mobile toolbar buttons show icons only, with accessible names', async ({ waffle }) => {
	const page = waffle.page;
	await expect(page.getByTestId('toolbar')).toBeVisible();
	const faces = await page.evaluate(() =>
		[...document.querySelectorAll('[data-testid="toolbar"] button')]
			.filter((b) => {
				const r = b.getBoundingClientRect();
				return r.width > 0 && r.height > 0 && b.dataset.testid;
			})
			.map((b) => {
				const label = b.querySelector('.tb-label');
				const lr = label?.getBoundingClientRect();
				return {
					id: b.dataset.testid,
					icons: b.querySelectorAll('svg.wi-icon').length,
					name: ((b.textContent ?? '').trim() || b.getAttribute('aria-label')) ?? '',
					labelShown: !!lr && lr.width > 1 && lr.height > 1,
					finish: b.dataset.testid === 'toolbar-btn-finish-sketch'
				};
			})
	);
	expect(faces.length).toBeGreaterThan(2);
	for (const f of faces) {
		if (f.id === 'toolbar-btn-rectangle-menu') continue;
		expect(f.icons, `${f.id} has no icon`).toBe(1);
		expect(f.name, `${f.id} has no accessible name`).not.toBe('');
		if (!f.finish) expect(f.labelShown, `${f.id} shows its label on mobile`).toBe(false);
	}
});
