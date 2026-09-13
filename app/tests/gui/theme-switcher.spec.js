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
		expect(await cssVar(page, '--model-color')).toBe('#d2d5da');
		// Edges are theme-driven too: near-white on the dark themes, near-black
		// here. The old value was a single hard-coded #222233 for every theme.
		expect(await cssVar(page, '--model-edge-color')).toBe('#10151c');
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

/**
 * Part edges are drawn ON the model faces and AGAINST the viewport ground, so
 * --model-edge-color has to clear both. Before it was a token it was a single
 * hard-coded #222233 for every theme — a dark-theme value that left silhouettes
 * at 1.1:1 against the default theme's own viewport ground.
 *
 * IMPORTANT: --model-color is NOT what the faces render as. The viewport's
 * lighting (Lighting.svelte: ambient 0.4 + key 0.8 + fill 0.3 + hemisphere)
 * multiplies it down in LINEAR space — measured at ~0.34-0.39x on the lit top
 * face and ~0.22x on the shaded side, with some highlight compression above
 * that. Contrast must be judged against the rendered shade; judging it from
 * the token is wrong by enough to INVERT a decision, and it did: near-black
 * edges on a light theme score 5.3:1 on the token and 1.3:1 in pixels.
 *
 * Measured off a canvas screenshot of an extruded box, 2026-09-13. Ratios are
 * the edge against the viewport ground, the lit top face, and the shaded side:
 *
 *   theme             token -> top / shaded      edge    grnd  top shad   vertex   min
 *   default           #8899aa -> #58626e #414c57  #f4f7fb 15.9  5.8  8.2  #ffffff  6.20
 *   solarized-dark    #7e9294 -> #515c61 #3b464a  #eee8d5 13.7  5.6  7.9  #fdf6e3  6.38
 *   monokai-dark      #8a8a7c -> #595650 #41413b  #f8f8f2 15.6  6.9  9.6  #ffffff  7.31
 *   retro             #3c4a40 -> #1f2721 #0f1712  #7dff5c 15.7 11.9 14.2  #b8ffa8 13.07
 *   witchhazel        #9a90b4 -> #625e74 #4a485d  #f8f8f2 10.9  5.8  8.3  #ffffff  6.23
 *   light             #d2d5da -> #828287 #696b70  #10151c 16.2  4.8  3.4  #000000  3.94
 *   solarized-light   #eee8d5 -> #8f8a85 #76736e  #073642 11.0  3.8  2.8  #002b36  3.18
 *   monokai-light     #dfdfd8 -> #888686 #706f6f  #272822 13.1  4.1  3.0  #15160f  3.63
 *
 * The last column is the VERTEX color's own worst ratio, always above the
 * edge's: points are 4px and unattenuated, so they get one step further out
 * the same ramp. That token replaced a hard-coded 0x666688 which the lighting
 * put at 1.03-1.33:1 of the rendered faces on seven of the eight themes —
 * corner markers invisible on the very part they mark (retro, at 2.79, was the
 * only one that worked, and only by accident).
 *
 * The light themes' --model-color was LIGHTENED to get there. With the old
 * mid-tone part (#7e8c9e and friends) the lighting rendered it as a dark slab
 * on a bright ground, so a dark edge vanished into the faces (1.3-1.9:1) and a
 * white one vanished into the ground (1.1:1); the ceiling was ~2.3:1 either
 * way. Lightening the part lifts the binding ratio to 2.75-3.44 AND keeps the
 * part itself clear of the ground (2.79-3.38:1), which is the other constraint
 * — the values are the joint optimum of the two, not a free choice. Solarized
 * stays inside its sixteen colors throughout: base2 part, base02 edge on
 * light; base2 edge on dark, where base3 would score 6.38 instead of 5.61 and
 * the palette's low-contrast philosophy is what declines the difference.
 *
 * This test can only see TOKENS, so it checks the two things a token says:
 * every theme defines the edge color, and it clears the viewport ground. The
 * face ratios above came from pixels and live in this comment as the record.
 */
test.describe('part edge contrast', () => {
	test('every theme defines edge and vertex colors that clear its viewport ground', async ({ waffle }) => {
		const { page } = waffle;
		await page.click(TRIGGER);
		const ids = await page.$$eval('[data-testid^="theme-option-"]', (els) =>
			els.map((e) => e.getAttribute('data-testid').replace('theme-option-', ''))
		);

		/** WCAG relative luminance of a #rrggbb string. */
		const relLum = (hex) => {
			const h = hex.replace('#', '');
			const ch = [0, 2, 4]
				.map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
				.map((x) => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4));
			return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
		};
		const ratio = (a, b) => {
			const [hi, lo] = [relLum(a), relLum(b)].sort((x, y) => y - x);
			return (hi + 0.05) / (lo + 0.05);
		};

		const report = {};
		for (const id of ids) {
			// Reading the ids above left the menu open, and its backdrop swallows
			// clicks on the trigger — so re-open only when a selection closed it.
			if ((await page.getAttribute(TRIGGER, 'aria-expanded')) !== 'true') {
				await page.click(TRIGGER);
			}
			await page.click(`[data-testid="theme-option-${id}"]`);
			const ground = await cssVar(page, '--viewport-bg');
			const edge = await cssVar(page, '--model-edge-color');
			const vertex = await cssVar(page, '--model-vertex-color');
			expect(`${id}: ${edge} ${vertex}`).toMatch(/: #[0-9a-f]{6} #[0-9a-f]{6}$/i);
			// Vertices are 4px points, so they must be at least as visible as the
			// 1px lines they sit on — each theme puts them one step further out
			// the same ramp. Same side of the faces, never the other one.
			report[id] = {
				edge: +ratio(edge, ground).toFixed(2),
				vertex: +ratio(vertex, ground).toFixed(2)
			};
			expect(`${id}: vertex ${relLum(vertex) >= relLum(edge) ? 'outruns' : 'trails'} edge`).toBe(
				`${id}: vertex ${relLum(edge) > 0.5 ? 'outruns' : 'trails'} edge`
			);
		}

		// Every theme clears 10 against its ground; 8 leaves room without going
		// vacuous. A failure prints the whole table, so the offending theme and
		// its actual ratios are visible at once.
		const failures = Object.entries(report).filter(([, r]) => r.edge < 8 || r.vertex < 8);
		expect(JSON.stringify({ failures: failures.map(([id]) => id), report }, null, 1)).toBe(
			JSON.stringify({ failures: [], report }, null, 1)
		);
		expect(Object.keys(report).length).toBe(ids.length);
	});
});
