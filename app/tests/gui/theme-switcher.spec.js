/**
 * Theme switcher tests — verifies the theme system:
 *   - default theme active on first load
 *   - selecting Retro sets data-theme, changes CSS variables, and persists
 *   - the choice survives a reload (anti-flash script applies it pre-paint)
 *   - switching back to Default clears the retro overrides
 *
 * The theme is pure CSS-variable swapping, so we assert on the resolved
 * `--text-primary` token and on `document.documentElement.dataset.theme`
 * rather than on pixels. No assertion-swallowing — waits throw on timeout.
 */
import { test, expect } from './helpers/waffle-test.js';

const TRIGGER = '[data-testid="theme-switcher-trigger"]';
const RETRO = '[data-testid="theme-option-retro"]';
const DEFAULT_OPT = '[data-testid="theme-option-default"]';

/** Resolved value of a CSS custom property on <html>, trimmed. */
async function cssVar(page, name) {
	return page.evaluate(
		(n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(),
		name
	);
}

async function dataTheme(page) {
	return page.evaluate(() => document.documentElement.dataset.theme || null);
}

async function storedTheme(page) {
	return page.evaluate(() => localStorage.getItem('waffle:theme'));
}

test.describe('theme switcher', () => {
	test('defaults to the default theme', async ({ waffle }) => {
		const { page } = waffle;
		expect(await dataTheme(page)).toBe('default');
		// Default token — the standard dark editor grey.
		expect(await cssVar(page, '--text-primary')).toBe('#cccccc');
		// Text on the (dark blue) accent status bar is white.
		expect(await cssVar(page, '--text-on-accent')).toBe('#ffffff');
	});

	test('selecting Retro applies phosphor-green tokens and persists', async ({ waffle }) => {
		const { page } = waffle;

		await page.click(TRIGGER);
		await expect(page.locator(RETRO)).toBeVisible();
		await page.click(RETRO);

		// data-theme + resolved tokens flip to retro.
		expect(await dataTheme(page)).toBe('retro');
		expect(await cssVar(page, '--text-primary')).toBe('#33ff66');
		expect(await cssVar(page, '--viewport-bg')).toBe('#05070a');
		// Retro's fills are bright, so text sitting on them flips to near-black
		// (fixes white-on-light-green in the status bar / error toasts).
		expect(await cssVar(page, '--text-on-accent')).toBe('#0b0f0b');
		// Persisted for next visit.
		expect(await storedTheme(page)).toBe('retro');
	});

	test('selecting Witch Hazel applies its violet palette and persists', async ({ waffle }) => {
		const { page } = waffle;

		await page.click(TRIGGER);
		const opt = page.locator('[data-testid="theme-option-witchhazel"]');
		await expect(opt).toBeVisible();
		await opt.click();

		expect(await dataTheme(page)).toBe('witchhazel');
		expect(await cssVar(page, '--text-primary')).toBe('#f8f8f2');
		expect(await cssVar(page, '--accent')).toBe('#c5a3ff');
		// Accent/success/error fills are light-to-bright, so text on them is dark.
		expect(await cssVar(page, '--text-on-accent')).toBe('#2b2740');
		expect(await storedTheme(page)).toBe('witchhazel');
	});

	test('retro survives a reload (applied before first paint)', async ({ waffle }) => {
		const { page } = waffle;

		await page.click(TRIGGER);
		await page.click(RETRO);
		expect(await dataTheme(page)).toBe('retro');

		await page.reload();
		// The inline anti-flash script sets this synchronously in <head>, so it
		// is correct even before the engine finishes booting.
		await expect(page.locator(TRIGGER)).toBeVisible();
		expect(await dataTheme(page)).toBe('retro');
		expect(await cssVar(page, '--text-primary')).toBe('#33ff66');
	});

	test('switching back to Default clears the retro overrides', async ({ waffle }) => {
		const { page } = waffle;

		await page.click(TRIGGER);
		await page.click(RETRO);
		expect(await cssVar(page, '--text-primary')).toBe('#33ff66');

		await page.click(TRIGGER);
		await expect(page.locator(DEFAULT_OPT)).toBeVisible();
		await page.click(DEFAULT_OPT);

		expect(await dataTheme(page)).toBe('default');
		expect(await cssVar(page, '--text-primary')).toBe('#cccccc');
		expect(await storedTheme(page)).toBe('default');
	});
});

/**
 * The five schemes added alongside the original three. Each asserts the four
 * tokens that carry the theme's identity: the page ground, the body text, the
 * accent, and the foreground that has to sit ON the accent (the token that
 * silently produces white-on-pale-yellow when a theme forgets it).
 */
const NEW_THEMES = [
	{ id: 'light', scheme: 'light', bg: '#ffffff', text: '#1f1f1f', accent: '#0067c0', onAccent: '#ffffff' },
	{ id: 'solarized-light', scheme: 'light', bg: '#fdf6e3', text: '#586e75', accent: '#268bd2', onAccent: '#002b36' },
	{ id: 'solarized-dark', scheme: 'dark', bg: '#002b36', text: '#93a1a1', accent: '#268bd2', onAccent: '#002b36' },
	{ id: 'monokai-light', scheme: 'light', bg: '#f8f8f2', text: '#272822', accent: '#0a7f9c', onAccent: '#f8f8f2' },
	{ id: 'monokai-dark', scheme: 'dark', bg: '#272822', text: '#f8f8f2', accent: '#66d9ef', onAccent: '#272822' },
];

test.describe('light-mode and editor-scheme themes', () => {
	for (const t of NEW_THEMES) {
		test(`${t.id} applies its palette, declares color-scheme: ${t.scheme}, and persists`, async ({ waffle }) => {
			const { page } = waffle;

			await page.click(TRIGGER);
			const opt = page.locator(`[data-testid="theme-option-${t.id}"]`);
			await expect(opt).toBeVisible();
			await opt.click();

			expect(await dataTheme(page)).toBe(t.id);
			expect(await cssVar(page, '--bg-primary')).toBe(t.bg);
			expect(await cssVar(page, '--text-primary')).toBe(t.text);
			expect(await cssVar(page, '--accent')).toBe(t.accent);
			expect(await cssVar(page, '--text-on-accent')).toBe(t.onAccent);

			// Native widgets (checkboxes, <input type=color>, carets, overlay
			// scrollbars) follow this, not the tokens.
			expect(
				await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme)
			).toBe(t.scheme);

			expect(await storedTheme(page)).toBe(t.id);
		});
	}

	test('the browser-chrome color follows the active theme', async ({ waffle }) => {
		const { page } = waffle;
		const chrome = () =>
			page.evaluate(() => document.querySelector('meta[name="theme-color"]')?.getAttribute('content'));

		await page.click(TRIGGER);
		await page.click('[data-testid="theme-option-light"]');
		expect(await chrome()).toBe('#ffffff');

		await page.click(TRIGGER);
		await page.click('[data-testid="theme-option-monokai-dark"]');
		expect(await chrome()).toBe('#272822');
	});

	test('a light theme reaches the viewport, not just the chrome', async ({ waffle }) => {
		const { page } = waffle;
		await page.click(TRIGGER);
		await page.click('[data-testid="theme-option-light"]');

		// The 3D stage and the solid's base color are theme tokens too — a theme
		// that only restyles the panels leaves a dark hole in the middle.
		expect(await cssVar(page, '--viewport-bg')).toBe('#eef1f5');
		expect(await cssVar(page, '--model-color')).toBe('#7e8c9e');
		// Sketch ink must be re-darkened for a light ground: the default theme's
		// #ffdd44 "selected" yellow is invisible on white.
		expect(await cssVar(page, '--sketch-selected')).toBe('#d98a00');
	});
});

/**
 * The contract app.css documents: a theme is a `:root[data-theme='<id>']` block
 * PLUS a THEMES entry. Registering the entry without the block is silent — the
 * theme just inherits the default `:root` values.
 *
 * For a DARK theme that is a legitimate shorthand (retro and Witch Hazel
 * deliberately inherit the baseline's sketch palette, which is already tuned
 * for a dark ground). For a LIGHT theme it is a bug every time: an inherited
 * token is a dark-theme color on a bright ground — #ffdd44 "selected" yellow
 * on white, a near-black viewport behind a white panel. So a theme that
 * declares `color-scheme: light` must redefine every color token.
 */
test.describe('theme registry', () => {
	test('every light theme overrides every color token', async ({ waffle }) => {
		const { page } = waffle;

		// The switcher renders one option per registered theme, so the menu IS
		// the registry as the UI sees it.
		await page.click(TRIGGER);
		await expect(page.locator('[data-testid="theme-switcher-menu"]')).toBeVisible();
		const ids = await page.$$eval('[data-testid^="theme-option-"]', (els) =>
			els.map((e) => e.getAttribute('data-testid').replace('theme-option-', ''))
		);
		expect(ids).toContain('default');
		expect(ids.length).toBe(NEW_THEMES.length + 3); // + default, retro, witchhazel

		const report = await page.evaluate((themeIds) => {
			// Browsers re-serialize selectorText (quote style, spacing), so match
			// on a normalized form rather than the literal source text.
			const norm = (sel) => sel.replace(/["']/g, '').replace(/\s+/g, '');

			/** Custom properties declared by the rule(s) matching `sel`. */
			const collect = (sel) => {
				const want = norm(sel);
				const out = new Map();
				for (const sheet of document.styleSheets) {
					let rules;
					try {
						rules = sheet.cssRules;
					} catch {
						continue; // cross-origin sheet
					}
					for (const rule of rules) {
						if (!rule.selectorText || norm(rule.selectorText) !== want) continue;
						for (const prop of rule.style) {
							out.set(prop, rule.style.getPropertyValue(prop).trim());
						}
					}
				}
				return out;
			};

			const base = collect(':root');
			// Layout and font tokens are theme-independent by design; only the
			// color tokens have to be redefined.
			const colorTokens = [...base.keys()].filter(
				(k) => k.startsWith('--') && !/^--(sai-|panel-width|toolbar-height|statusbar-height|font-)/.test(k)
			);

			const missing = {};
			const schemes = {};
			for (const id of themeIds) {
				if (id === 'default') continue; // the baseline itself
				const own = collect(`:root[data-theme='${id}']`);
				if (own.size === 0) {
					missing[id] = ['<no :root[data-theme] block at all>'];
					continue;
				}
				// Every theme states which way it leans — that is what drives the
				// native widgets, and it is what decides the rule below.
				schemes[id] = own.get('color-scheme') || '<undeclared>';
				if (schemes[id] !== 'light') continue;
				const gaps = colorTokens.filter((k) => !own.has(k));
				if (gaps.length) missing[id] = gaps;
			}
			return { colorTokenCount: colorTokens.length, missing, schemes };
		}, ids);

		// A non-empty map names the theme and the exact tokens it forgot.
		expect(report.missing).toEqual({});
		// Guard the guard: if the token list ever collapses to nothing, the
		// check above passes vacuously.
		expect(report.colorTokenCount).toBeGreaterThanOrEqual(26);
		// And every registered theme declares its lean, so none falls through to
		// the browser default.
		for (const [id, scheme] of Object.entries(report.schemes)) {
			expect(`${id}: ${scheme}`).toMatch(/: (light|dark)$/);
		}
		expect(Object.values(report.schemes).filter((s) => s === 'light').length).toBe(
			NEW_THEMES.filter((t) => t.scheme === 'light').length
		);
	});
});
