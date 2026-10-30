/**
 * Toolbar and menu icons, desktop (ui-chrome M13, specs/ui_toolbar_icons.md).
 *
 * User decisions 2026-10-30: on desktop every button shows its icon AND its
 * label; fillet / chamfer / shell look disabled; constraint buttons are
 * pictures. Each check walks the buttons that are actually rendered (inline,
 * and inside the dropdowns once opened) rather than a hard-coded list, so a
 * new button without an icon fails here.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch } from './helpers/toolbar.js';

/** Every visible toolbar button under `root`, with what it shows. */
async function buttonFaces(page, rootSelector) {
	return page.evaluate((sel) => {
		const root = document.querySelector(sel);
		if (!root) return null;
		// Buttons with a testid: the project-name rename button is text by design.
		return [...root.querySelectorAll('button[data-testid]')]
			.filter((b) => {
				const r = b.getBoundingClientRect();
				return r.width > 0 && r.height > 0;
			})
			.map((b) => {
				const label = b.querySelector('.tb-label, .cb-label');
				const lr = label?.getBoundingClientRect();
				return {
					id: b.dataset.testid,
					icons: b.querySelectorAll('svg.wi-icon').length,
					text: (b.textContent ?? '').trim(),
					aria: b.getAttribute('aria-label') ?? '',
					labelVisible: !!lr && lr.width > 1 && lr.height > 1
				};
			});
	}, rootSelector);
}

/** Buttons whose face is a bare glyph by design (no text label). */
const GLYPH_ONLY = new Set(['toolbar-btn-settings', 'toolbar-btn-overflow', 'toolbar-btn-rectangle-menu']);

function expectIconAndLabel(faces, where, minButtons = 4) {
	expect(faces, `${where}: toolbar not found`).not.toBeNull();
	expect(faces.length, `${where}: too few buttons`).toBeGreaterThanOrEqual(minButtons);
	for (const f of faces) {
		if (f.id === 'toolbar-btn-rectangle-menu') continue;
		expect(f.icons, `${where}: ${f.id} has no icon`).toBe(1);
		if (GLYPH_ONLY.has(f.id)) {
			expect(f.aria, `${where}: ${f.id} needs an aria-label`).not.toBe('');
		} else {
			expect(f.labelVisible, `${where}: ${f.id} label not visible`).toBe(true);
			expect(f.text, `${where}: ${f.id} has no label text`).not.toBe('');
		}
	}
}

test.describe('toolbar icons (desktop)', () => {
	for (const width of [1280, 1920]) {
		test(`every modeling-mode button shows an icon and a label @${width}`, async ({ waffle }) => {
			const page = waffle.page;
			await page.setViewportSize({ width, height: 900 });
			await expect(page.getByTestId('toolbar')).toBeVisible();
			expectIconAndLabel(await buttonFaces(page, '[data-testid="toolbar"]'), `part @${width}`);

			const more = page.getByTestId('toolbar-btn-more-tools');
			if (await more.isVisible()) {
				await more.click();
				await expect(page.getByTestId('more-tools-dropdown')).toBeVisible();
				expectIconAndLabel(await buttonFaces(page, '[data-testid="more-tools-dropdown"]'), `part More @${width}`, 1);
			}
		});

		test(`every sketch-mode button and constraint shows an icon @${width}`, async ({ waffle }) => {
			const page = waffle.page;
			await page.setViewportSize({ width, height: 900 });
			await clickSketch(page);
			expectIconAndLabel(await buttonFaces(page, '[data-testid="toolbar"]'), `sketch @${width}`);

			await page.getByTestId('toolbar-btn-constraints-dropdown').click();
			await expect(page.getByTestId('constraints-dropdown')).toBeVisible();
			const faces = await buttonFaces(page, '[data-testid="constraints-dropdown"]');
			expect(faces.length).toBe(15);
			for (const f of faces) {
				expect(f.icons, `${f.id} has no picture`).toBe(1);
				expect(f.text, `${f.id} has no name`).not.toBe('');
			}
		});
	}

	test('the ⋮ menu items carry icons, and view toggles report their state', async ({ waffle }) => {
		const page = waffle.page;
		await page.setViewportSize({ width: 1100, height: 900 });
		await page.getByTestId('toolbar-btn-overflow').click();
		const menu = page.getByTestId('toolbar-overflow-menu');
		await expect(menu).toBeVisible();
		const items = await buttonFaces(page, '[data-testid="toolbar-overflow-menu"]');
		expect(items.length).toBeGreaterThan(5);
		for (const f of items) expect(f.icons, `${f.id} has no icon`).toBeGreaterThanOrEqual(1);
		const planes = menu.getByTestId('toolbar-btn-toggle-planes');
		if (await planes.isVisible()) {
			await expect(planes).toHaveAttribute('role', 'menuitemcheckbox');
			await expect(planes).toHaveAttribute('aria-checked', /true|false/);
		}
	});

	test('fillet, chamfer and shell look disabled and say so', async ({ waffle }) => {
		const page = waffle.page;
		await page.setViewportSize({ width: 3200, height: 900 });
		for (const id of ['fillet', 'chamfer', 'shell']) {
			const btn = page.getByTestId(`toolbar-btn-${id}`);
			await expect(btn).toBeVisible();
			await expect(btn).toHaveClass(/\bdeferred\b/);
			await expect(btn).toHaveAttribute('title', /not yet available/);
		}
		const extrude = page.getByTestId('toolbar-btn-extrude');
		await expect(extrude).not.toHaveClass(/\bdeferred\b/);
	});
});
